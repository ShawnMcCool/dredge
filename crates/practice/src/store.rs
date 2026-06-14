use crate::error::Result;
use crate::model::{
    Analysis, LoopId, LoopKind, LoopRegion, Plan, PlanId, PlanStep, Rating, Section, SectionId,
    Song, SongId,
};
use crate::schedule::Resurfacing;
use rusqlite::params;
use time::macros::format_description;

const SCHEMA_V1: &str = "
CREATE TABLE songs (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    artist TEXT,
    path TEXT NOT NULL,
    file_hash TEXT NOT NULL UNIQUE,
    duration_secs REAL NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE sections (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    start_secs REAL NOT NULL,
    end_secs REAL NOT NULL,
    position INTEGER NOT NULL
);
CREATE TABLE loops (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    start_secs REAL NOT NULL,
    end_secs REAL NOT NULL,
    kind_json TEXT NOT NULL
);
CREATE TABLE plans (
    id INTEGER PRIMARY KEY,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    steps_json TEXT NOT NULL
);
CREATE TABLE reps (
    id INTEGER PRIMARY KEY,
    loop_id INTEGER NOT NULL REFERENCES loops(id) ON DELETE CASCADE,
    plan_id INTEGER REFERENCES plans(id) ON DELETE SET NULL,
    mode TEXT NOT NULL,
    rate REAL NOT NULL,
    rating TEXT,
    is_retest INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE resurfacing (
    loop_id INTEGER PRIMARY KEY REFERENCES loops(id) ON DELETE CASCADE,
    interval_idx INTEGER NOT NULL,
    due_on TEXT NOT NULL
);
";

/// v2: cached analysis results (one row per song, JSON columns).
const SCHEMA_V2: &str = "
CREATE TABLE analysis (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    bpm REAL,
    beats_json TEXT NOT NULL,
    downbeats_json TEXT NOT NULL,
    sections_json TEXT NOT NULL,
    engine TEXT NOT NULL
);
";

/// v3: durable app settings (arbitrary JSON values per key).
const SCHEMA_V3: &str = "
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
";

/// v4: per-operation profiling runs (heavy ops). `stages` is JSON.
const SCHEMA_V4: &str = "
CREATE TABLE profiles (
    id INTEGER PRIMARY KEY,
    op TEXT NOT NULL,
    song_id INTEGER,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    total_ms INTEGER NOT NULL,
    ok INTEGER NOT NULL,
    error TEXT,
    device TEXT,
    engine TEXT,
    stages_json TEXT NOT NULL
);
";

/// v5: per-run max resource metrics on profiles.
const SCHEMA_V5: &str = "
ALTER TABLE profiles ADD COLUMN max_cpu_pct INTEGER;
ALTER TABLE profiles ADD COLUMN max_gpu_util INTEGER;
ALTER TABLE profiles ADD COLUMN max_vram_used_mb INTEGER;
ALTER TABLE profiles ADD COLUMN vram_total_mb INTEGER;
";

/// v6: optional manual name override on loops (NULL = dynamic name).
const SCHEMA_V6: &str = "
ALTER TABLE loops ADD COLUMN name_override TEXT;
";

pub struct Store {
    conn: rusqlite::Connection,
}

pub struct NewSong<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub path: &'a str,
    pub file_hash: &'a str,
    pub duration_secs: f64,
}

pub struct NewSection<'a> {
    pub name: &'a str,
    pub start: f64,
    pub end: f64,
    pub position: i32,
}

pub struct NewLoop<'a> {
    pub name: &'a str,
    pub name_override: Option<&'a str>,
    pub start: f64,
    pub end: f64,
    pub kind: LoopKind,
}

pub struct NewRep {
    pub loop_id: LoopId,
    pub plan_id: Option<PlanId>,
    pub mode: String,
    pub rate: f64,
    pub rating: Option<Rating>,
    pub is_retest: bool,
}

fn rating_to_str(r: Rating) -> &'static str {
    match r {
        Rating::Miss => "miss",
        Rating::Shaky => "shaky",
        Rating::Solid => "solid",
    }
}

fn rating_from_str(s: &str) -> rusqlite::Result<Rating> {
    match s {
        "miss" => Ok(Rating::Miss),
        "shaky" => Ok(Rating::Shaky),
        "solid" => Ok(Rating::Solid),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown rating: {other}").into(),
        )),
    }
}

fn json_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

fn date_err(e: time::error::Parse) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

const DATE_FMT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Self::init(rusqlite::Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(rusqlite::Connection::open_in_memory()?)
    }

    fn init(conn: rusqlite::Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < 1 {
            self.conn.execute_batch(SCHEMA_V1)?;
            self.conn.pragma_update(None, "user_version", 1)?;
        }
        if version < 2 {
            self.conn.execute_batch(SCHEMA_V2)?;
            self.conn.pragma_update(None, "user_version", 2)?;
        }
        if version < 3 {
            self.conn.execute_batch(SCHEMA_V3)?;
            self.conn.pragma_update(None, "user_version", 3)?;
        }
        if version < 4 {
            self.conn.execute_batch(SCHEMA_V4)?;
            self.conn.pragma_update(None, "user_version", 4)?;
        }
        if version < 5 {
            self.conn.execute_batch(SCHEMA_V5)?;
            self.conn.pragma_update(None, "user_version", 5)?;
        }
        if version < 6 {
            self.conn.execute_batch(SCHEMA_V6)?;
            self.conn.pragma_update(None, "user_version", 6)?;
        }
        Ok(())
    }

    pub fn insert_song(&self, s: NewSong) -> Result<Song> {
        self.conn.execute(
            "INSERT INTO songs (title, artist, path, file_hash, duration_secs)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![s.title, s.artist, s.path, s.file_hash, s.duration_secs],
        )?;
        Ok(Song {
            id: SongId(self.conn.last_insert_rowid()),
            title: s.title.to_owned(),
            artist: s.artist.map(str::to_owned),
            path: s.path.to_owned(),
            file_hash: s.file_hash.to_owned(),
            duration_secs: s.duration_secs,
        })
    }

    fn song_from_row(row: &rusqlite::Row) -> rusqlite::Result<Song> {
        Ok(Song {
            id: SongId(row.get(0)?),
            title: row.get(1)?,
            artist: row.get(2)?,
            path: row.get(3)?,
            file_hash: row.get(4)?,
            duration_secs: row.get(5)?,
        })
    }

    pub fn song_by_hash(&self, hash: &str) -> Result<Option<Song>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, path, file_hash, duration_secs
             FROM songs WHERE file_hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![hash], Self::song_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn list_songs(&self) -> Result<Vec<Song>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, path, file_hash, duration_secs
             FROM songs ORDER BY id",
        )?;
        let songs = stmt
            .query_map([], Self::song_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(songs)
    }

    pub fn song_by_id(&self, id: SongId) -> Result<Option<Song>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, path, file_hash, duration_secs
             FROM songs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id.0], Self::song_from_row)?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn delete_song(&self, id: SongId) -> Result<()> {
        // profiles.song_id has no FK (it's nullable — some runs aren't
        // song-scoped), so cascade won't reach it; clear this song's rows
        // explicitly. Everything else cascades via ON DELETE CASCADE.
        self.conn
            .execute("DELETE FROM profiles WHERE song_id = ?1", params![id.0])?;
        self.conn
            .execute("DELETE FROM songs WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    pub fn update_song(&self, id: SongId, title: &str, artist: Option<&str>) -> Result<Song> {
        self.conn.execute(
            "UPDATE songs SET title = ?1, artist = ?2 WHERE id = ?3",
            params![title, artist, id.0],
        )?;
        self.song_by_id(id)?.ok_or(crate::error::Error::NotFound)
    }

    /// Replace all sections for a song atomically (UI saves whole lane).
    pub fn replace_sections(
        &mut self,
        song_id: SongId,
        sections: &[NewSection],
    ) -> Result<Vec<Section>> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM sections WHERE song_id = ?1",
            params![song_id.0],
        )?;
        let mut out = Vec::with_capacity(sections.len());
        for s in sections {
            tx.execute(
                "INSERT INTO sections (song_id, name, start_secs, end_secs, position)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![song_id.0, s.name, s.start, s.end, s.position],
            )?;
            out.push(Section {
                id: SectionId(tx.last_insert_rowid()),
                song_id,
                name: s.name.to_owned(),
                start: s.start,
                end: s.end,
                position: s.position,
            });
        }
        tx.commit()?;
        out.sort_by_key(|s| s.position);
        Ok(out)
    }

    pub fn list_sections(&self, song_id: SongId) -> Result<Vec<Section>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, song_id, name, start_secs, end_secs, position
             FROM sections WHERE song_id = ?1 ORDER BY position",
        )?;
        let sections = stmt
            .query_map(params![song_id.0], |row| {
                Ok(Section {
                    id: SectionId(row.get(0)?),
                    song_id: SongId(row.get(1)?),
                    name: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
                    position: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sections)
    }

    pub fn insert_loop(&self, song_id: SongId, l: NewLoop) -> Result<LoopRegion> {
        let kind_json = serde_json::to_string(&l.kind)?;
        self.conn.execute(
            "INSERT INTO loops (song_id, name, name_override, start_secs, end_secs, kind_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![song_id.0, l.name, l.name_override, l.start, l.end, kind_json],
        )?;
        Ok(LoopRegion {
            id: LoopId(self.conn.last_insert_rowid()),
            song_id,
            name: l.name.to_owned(),
            name_override: l.name_override.map(str::to_owned),
            start: l.start,
            end: l.end,
            kind: l.kind,
        })
    }

    pub fn loop_by_id(&self, id: LoopId) -> Result<Option<LoopRegion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, song_id, name, name_override, start_secs, end_secs, kind_json
             FROM loops WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id.0], |row| {
            let kind_json: String = row.get(6)?;
            Ok(LoopRegion {
                id: LoopId(row.get(0)?),
                song_id: SongId(row.get(1)?),
                name: row.get(2)?,
                name_override: row.get(3)?,
                start: row.get(4)?,
                end: row.get(5)?,
                kind: serde_json::from_str(&kind_json).map_err(json_err)?,
            })
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    /// Rename and/or move a loop in place; kind is untouched. `name` is the
    /// effective display name; `name_override` is the pinned manual name (NULL
    /// reverts to dynamic).
    pub fn update_loop(
        &self,
        id: LoopId,
        name: &str,
        name_override: Option<&str>,
        start: f64,
        end: f64,
    ) -> Result<LoopRegion> {
        self.conn.execute(
            "UPDATE loops SET name = ?2, name_override = ?3, start_secs = ?4, end_secs = ?5
             WHERE id = ?1",
            params![id.0, name, name_override, start, end],
        )?;
        self.loop_by_id(id)?.ok_or(crate::error::Error::NotFound)
    }

    pub fn delete_loop(&self, id: LoopId) -> Result<()> {
        self.conn
            .execute("DELETE FROM loops WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    pub fn list_loops(&self, song_id: SongId) -> Result<Vec<LoopRegion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, song_id, name, name_override, start_secs, end_secs, kind_json
             FROM loops WHERE song_id = ?1 ORDER BY id",
        )?;
        let loops = stmt
            .query_map(params![song_id.0], |row| {
                let kind_json: String = row.get(6)?;
                Ok(LoopRegion {
                    id: LoopId(row.get(0)?),
                    song_id: SongId(row.get(1)?),
                    name: row.get(2)?,
                    name_override: row.get(3)?,
                    start: row.get(4)?,
                    end: row.get(5)?,
                    kind: serde_json::from_str(&kind_json).map_err(json_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(loops)
    }

    pub fn save_plan(&self, song_id: SongId, name: &str, steps: &[PlanStep]) -> Result<Plan> {
        let steps_json = serde_json::to_string(steps)?;
        self.conn.execute(
            "INSERT INTO plans (song_id, name, steps_json) VALUES (?1, ?2, ?3)",
            params![song_id.0, name, steps_json],
        )?;
        Ok(Plan {
            id: PlanId(self.conn.last_insert_rowid()),
            song_id,
            name: name.to_owned(),
            steps: steps.to_vec(),
        })
    }

    pub fn list_plans(&self, song_id: SongId) -> Result<Vec<Plan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, song_id, name, steps_json FROM plans WHERE song_id = ?1 ORDER BY id",
        )?;
        let plans = stmt
            .query_map(params![song_id.0], |row| {
                let steps_json: String = row.get(3)?;
                Ok(Plan {
                    id: PlanId(row.get(0)?),
                    song_id: SongId(row.get(1)?),
                    name: row.get(2)?,
                    steps: serde_json::from_str(&steps_json).map_err(json_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(plans)
    }

    pub fn record_rep(&self, r: NewRep) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reps (loop_id, plan_id, mode, rate, rating, is_retest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                r.loop_id.0,
                r.plan_id.map(|p| p.0),
                r.mode,
                r.rate,
                r.rating.map(rating_to_str),
                r.is_retest,
            ],
        )?;
        Ok(())
    }

    pub fn upsert_resurfacing(&self, r: Resurfacing) -> Result<()> {
        let due_on = r
            .due_on
            .format(DATE_FMT)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.conn.execute(
            "INSERT INTO resurfacing (loop_id, interval_idx, due_on) VALUES (?1, ?2, ?3)
             ON CONFLICT(loop_id) DO UPDATE SET interval_idx = ?2, due_on = ?3",
            params![r.loop_id.0, r.interval_idx as i64, due_on],
        )?;
        Ok(())
    }

    pub fn all_resurfacing(&self) -> Result<Vec<Resurfacing>> {
        let mut stmt = self
            .conn
            .prepare("SELECT loop_id, interval_idx, due_on FROM resurfacing ORDER BY loop_id")?;
        let items = stmt
            .query_map([], |row| {
                let due_on: String = row.get(2)?;
                Ok(Resurfacing {
                    loop_id: LoopId(row.get(0)?),
                    interval_idx: row.get::<_, i64>(1)? as usize,
                    due_on: time::Date::parse(&due_on, DATE_FMT).map_err(date_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(items)
    }

    /// Upsert the cached analysis for a song (re-analysis overwrites).
    pub fn save_analysis(&self, song_id: SongId, a: &Analysis) -> Result<()> {
        self.conn.execute(
            "INSERT INTO analysis (song_id, bpm, beats_json, downbeats_json, sections_json, engine)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(song_id) DO UPDATE SET bpm = ?2, beats_json = ?3,
                 downbeats_json = ?4, sections_json = ?5, engine = ?6",
            params![
                song_id.0,
                a.bpm,
                serde_json::to_string(&a.beats)?,
                serde_json::to_string(&a.downbeats)?,
                serde_json::to_string(&a.sections)?,
                a.engine,
            ],
        )?;
        Ok(())
    }

    pub fn get_analysis(&self, song_id: SongId) -> Result<Option<Analysis>> {
        let mut stmt = self.conn.prepare(
            "SELECT bpm, beats_json, downbeats_json, sections_json, engine
             FROM analysis WHERE song_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![song_id.0], |row| {
            let beats: String = row.get(1)?;
            let downbeats: String = row.get(2)?;
            let sections: String = row.get(3)?;
            Ok(Analysis {
                bpm: row.get(0)?,
                beats: serde_json::from_str(&beats).map_err(json_err)?,
                downbeats: serde_json::from_str(&downbeats).map_err(json_err)?,
                sections: serde_json::from_str(&sections).map_err(json_err)?,
                engine: row.get(4)?,
            })
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    /// Upsert one durable setting (arbitrary JSON value).
    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = ?2",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value_json FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| {
            let v: String = row.get(0)?;
            serde_json::from_str(&v).map_err(json_err)
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn all_settings(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value_json FROM settings ORDER BY key")?;
        let rows = stmt
            .query_map([], |row| {
                let v: String = row.get(1)?;
                Ok((
                    row.get::<_, String>(0)?,
                    serde_json::from_str(&v).map_err(json_err)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Insert one profiling run; trims history to the most recent 200.
    /// Returns the `started_at` SQLite assigned.
    pub fn save_profile(&self, run: &crate::model::ProfileRun) -> Result<String> {
        let started: String = self.conn.query_row(
            "INSERT INTO profiles (op, song_id, total_ms, ok, error, device, engine, stages_json,
                max_cpu_pct, max_gpu_util, max_vram_used_mb, vram_total_mb)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             RETURNING started_at",
            params![
                run.op,
                run.song_id.map(|s| s.0),
                run.total_ms as i64,
                run.ok as i64,
                run.error,
                run.device,
                run.engine,
                serde_json::to_string(&run.stages)?,
                run.max_cpu_pct.map(|v| v as i64),
                run.max_gpu_util.map(|v| v as i64),
                run.max_vram_used_mb.map(|v| v as i64),
                run.vram_total_mb.map(|v| v as i64),
            ],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "DELETE FROM profiles WHERE id NOT IN
                (SELECT id FROM profiles ORDER BY id DESC LIMIT 200)",
            [],
        )?;
        Ok(started)
    }

    pub fn list_profiles(&self, limit: i64) -> Result<Vec<crate::model::ProfileRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT op, song_id, started_at, total_ms, ok, error, device, engine, stages_json,
                max_cpu_pct, max_gpu_util, max_vram_used_mb, vram_total_mb
             FROM profiles ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let stages: String = row.get(8)?;
                Ok(crate::model::ProfileRun {
                    op: row.get(0)?,
                    song_id: row.get::<_, Option<i64>>(1)?.map(crate::model::SongId),
                    started_at: row.get(2)?,
                    total_ms: row.get::<_, i64>(3)? as u64,
                    ok: row.get::<_, i64>(4)? != 0,
                    error: row.get(5)?,
                    device: row.get(6)?,
                    engine: row.get(7)?,
                    max_cpu_pct: row.get::<_, Option<i64>>(9)?.map(|v| v as u32),
                    max_gpu_util: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
                    max_vram_used_mb: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
                    vram_total_mb: row.get::<_, Option<i64>>(12)?.map(|v| v as u32),
                    stages: serde_json::from_str(&stages).map_err(json_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Latest retest rating per loop — the retention metric.
    pub fn retention(&self, song_id: SongId) -> Result<Vec<(LoopId, Rating, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.loop_id, r.rating, r.created_at FROM reps r
             JOIN loops l ON l.id = r.loop_id
             WHERE l.song_id = ?1 AND r.is_retest = 1 AND r.rating IS NOT NULL
               AND r.id = (SELECT MAX(id) FROM reps r2 WHERE r2.loop_id = r.loop_id AND r2.is_retest = 1 AND r2.rating IS NOT NULL)",
        )?;
        let rows = stmt
            .query_map(params![song_id.0], |row| {
                let rating: String = row.get(1)?;
                Ok((
                    LoopId(row.get(0)?),
                    rating_from_str(&rating)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_roundtrip_and_trim() {
        let store = Store::open_in_memory().unwrap();
        let run = crate::model::ProfileRun {
            op: "analysis".into(),
            song_id: Some(crate::model::SongId(7)),
            started_at: String::new(),
            total_ms: 1234,
            ok: true,
            error: None,
            device: Some("cpu".into()),
            engine: Some("songformer".into()),
            max_cpu_pct: Some(496),
            max_gpu_util: Some(41),
            max_vram_used_mb: Some(6100),
            vram_total_mb: Some(16000),
            stages: vec![crate::model::ProfileStage {
                name: "analyze".into(),
                ms: 1234,
                note: None,
            }],
        };
        let started = store.save_profile(&run).unwrap();
        assert!(!started.is_empty(), "store assigns a timestamp");

        let listed = store.list_profiles(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].op, "analysis");
        assert_eq!(listed[0].total_ms, 1234);
        assert_eq!(listed[0].engine.as_deref(), Some("songformer"));
        assert_eq!(listed[0].stages.len(), 1);
        assert!(!listed[0].started_at.is_empty());
        assert_eq!(listed[0].max_cpu_pct, Some(496));
        assert_eq!(listed[0].max_gpu_util, Some(41));
        assert_eq!(listed[0].max_vram_used_mb, Some(6100));
        assert_eq!(listed[0].vram_total_mb, Some(16000));

        // trim keeps only the most recent 200
        for i in 0..205 {
            let mut r = run.clone();
            r.total_ms = i;
            store.save_profile(&r).unwrap();
        }
        assert_eq!(store.list_profiles(1000).unwrap().len(), 200);
    }

    #[test]
    fn delete_song_clears_its_profiles_only() {
        let store = Store::open_in_memory().unwrap();
        let a = store
            .insert_song(NewSong {
                title: "A",
                artist: None,
                path: "/a",
                file_hash: "ha",
                duration_secs: 1.0,
            })
            .unwrap();
        let b = store
            .insert_song(NewSong {
                title: "B",
                artist: None,
                path: "/b",
                file_hash: "hb",
                duration_secs: 1.0,
            })
            .unwrap();

        let mk = |sid: Option<SongId>| crate::model::ProfileRun {
            op: "analysis".into(),
            song_id: sid,
            started_at: String::new(),
            total_ms: 1,
            ok: true,
            error: None,
            device: None,
            engine: None,
            max_cpu_pct: None,
            max_gpu_util: None,
            max_vram_used_mb: None,
            vram_total_mb: None,
            stages: vec![],
        };
        store.save_profile(&mk(Some(a.id))).unwrap();
        store.save_profile(&mk(Some(b.id))).unwrap();
        store.save_profile(&mk(None)).unwrap(); // not song-scoped

        store.delete_song(a.id).unwrap();

        let left = store.list_profiles(100).unwrap();
        assert!(
            !left.iter().any(|p| p.song_id.map(|s| s.0) == Some(a.id.0)),
            "deleted song's profiles are cleared"
        );
        assert!(
            left.iter().any(|p| p.song_id.map(|s| s.0) == Some(b.id.0)),
            "another song's profiles are kept"
        );
        assert!(
            left.iter().any(|p| p.song_id.is_none()),
            "non-song-scoped profiles are kept"
        );
        assert!(
            store.song_by_id(b.id).unwrap().is_some(),
            "other song untouched"
        );
    }
}
