use crate::error::Result;
use rusqlite::params;

const SCHEMA_V1: &str = "
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL
);
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
    max_cpu_pct INTEGER,
    max_gpu_util INTEGER,
    max_vram_used_mb INTEGER,
    vram_total_mb INTEGER,
    stages_json TEXT NOT NULL
);
";

fn json_err(e: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
}

pub struct Store {
    conn: rusqlite::Connection,
}

impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        Self::init(rusqlite::Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(rusqlite::Connection::open_in_memory()?)
    }

    fn init(conn: rusqlite::Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Durability/concurrency tuning. WAL + synchronous=NORMAL collapses the
        // ~2 fsyncs-per-write of the default rollback journal down to far fewer,
        // which is the single biggest write-latency lever for this single-writer
        // desktop DB. (WAL is a no-op for :memory: connections used in tests.)
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "cache_size", -8000)?; // ~8 MB page cache
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
        Ok(())
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
            .prepare_cached("SELECT value_json FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| {
            let v: String = row.get(0)?;
            serde_json::from_str(&v).map_err(json_err)
        })?;
        rows.next().transpose().map_err(Into::into)
    }

    pub fn all_settings(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT key, value_json FROM settings ORDER BY key")?;
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
        // Cap the profiles table at 200 rows, but only pay for the trim once we
        // actually exceed it (this runs after every heavy op). The `id <= …`
        // form is index-friendly (PK), unlike the old `NOT IN (subquery)` which
        // re-scanned the whole table on every single insert.
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))?;
        if count > 200 {
            self.conn.execute(
                "DELETE FROM profiles WHERE id <=
                    (SELECT id FROM profiles ORDER BY id DESC LIMIT 1 OFFSET 200)",
                [],
            )?;
        }
        Ok(started)
    }

    pub fn list_profiles(&self, limit: i64) -> Result<Vec<crate::model::ProfileRun>> {
        let mut stmt = self.conn.prepare_cached(
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
    fn save_profile_caps_table_at_200_keeping_newest() {
        let store = Store::open_in_memory().unwrap();
        let mk = || crate::model::ProfileRun {
            op: "analysis".into(),
            song_id: None,
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
        for _ in 0..205 {
            store.save_profile(&mk()).unwrap();
        }
        let count: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 200, "profiles table is capped at 200 rows");
        // The newest rows survive: ids 6..=205 remain, 1..=5 were trimmed.
        let min_id: i64 = store
            .conn
            .query_row("SELECT MIN(id) FROM profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(min_id, 6, "the 5 oldest rows were trimmed");
    }

    #[test]
    fn settings_roundtrip_arbitrary_json() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.get_setting("ui_scale").unwrap().is_none());
        assert!(store.all_settings().unwrap().is_empty());

        store
            .set_setting("ui_scale", &serde_json::json!(1.75))
            .unwrap();
        store
            .set_setting("grid_snap_default", &serde_json::json!(false))
            .unwrap();
        assert_eq!(
            store.get_setting("ui_scale").unwrap(),
            Some(serde_json::json!(1.75))
        );

        // upsert overwrites in place
        store
            .set_setting("ui_scale", &serde_json::json!(2.0))
            .unwrap();
        assert_eq!(
            store.all_settings().unwrap(),
            vec![
                ("grid_snap_default".into(), serde_json::json!(false)),
                ("ui_scale".into(), serde_json::json!(2.0)),
            ]
        );
    }
}
