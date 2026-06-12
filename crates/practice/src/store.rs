use crate::error::Result;
use crate::model::{
    LoopId, LoopKind, LoopRegion, Plan, PlanId, PlanStep, Rating, Section, SectionId, Song, SongId,
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
        let version: i64 =
            self.conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            self.conn.execute_batch(SCHEMA_V1)?;
            self.conn.pragma_update(None, "user_version", 1)?;
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

    pub fn delete_song(&self, id: SongId) -> Result<()> {
        self.conn.execute("DELETE FROM songs WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    /// Replace all sections for a song atomically (UI saves whole lane).
    pub fn replace_sections(
        &mut self,
        song_id: SongId,
        sections: &[NewSection],
    ) -> Result<Vec<Section>> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM sections WHERE song_id = ?1", params![song_id.0])?;
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
            "INSERT INTO loops (song_id, name, start_secs, end_secs, kind_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![song_id.0, l.name, l.start, l.end, kind_json],
        )?;
        Ok(LoopRegion {
            id: LoopId(self.conn.last_insert_rowid()),
            song_id,
            name: l.name.to_owned(),
            start: l.start,
            end: l.end,
            kind: l.kind,
        })
    }

    pub fn delete_loop(&self, id: LoopId) -> Result<()> {
        self.conn.execute("DELETE FROM loops WHERE id = ?1", params![id.0])?;
        Ok(())
    }

    pub fn list_loops(&self, song_id: SongId) -> Result<Vec<LoopRegion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, song_id, name, start_secs, end_secs, kind_json
             FROM loops WHERE song_id = ?1 ORDER BY id",
        )?;
        let loops = stmt
            .query_map(params![song_id.0], |row| {
                let kind_json: String = row.get(5)?;
                Ok(LoopRegion {
                    id: LoopId(row.get(0)?),
                    song_id: SongId(row.get(1)?),
                    name: row.get(2)?,
                    start: row.get(3)?,
                    end: row.get(4)?,
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
        let due_on = r.due_on.format(DATE_FMT).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
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
