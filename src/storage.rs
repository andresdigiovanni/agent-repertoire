use crate::models::{Language, ToolRecord};
use crate::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Duration;

const TOOL_COLS: &str = "id, name, description, language, entrypoint, input_schema, capabilities, keywords, commands, examples, timeout_seconds, version, created_at, updated_at, usage_count, last_used_at";

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS tools (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    language TEXT NOT NULL,
    entrypoint TEXT NOT NULL,
    input_schema TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '{}',
    keywords TEXT NOT NULL DEFAULT '[]',
    commands TEXT NOT NULL DEFAULT '[]',
    examples TEXT NOT NULL DEFAULT '[]',
    timeout_seconds INTEGER,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    last_used_at TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS tools_fts USING fts5(
    name, description, keywords, commands, tokenize = 'unicode61'
);
CREATE UNIQUE INDEX IF NOT EXISTS schema_version_version_unique ON schema_version(version);
";

fn parse_language(s: &str) -> Language {
    match s {
        "python" => Language::Python,
        _ => Language::Bash,
    }
}

fn parse_string_array(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

#[derive(Debug)]
pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.execute_batch(SCHEMA_SQL)?;
        let min_version: Option<i64> = conn
            .query_row(
                "SELECT version FROM schema_version ORDER BY version LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(v) = min_version {
            if v < 2 {
                return Err(crate::RepError::Drift(format!(
                    "catalog schema v{} is incompatible with this version; delete the data root (or move it aside) and recreate your tools",
                    v
                )));
            }
            if v == 2 {
                conn.execute_batch(
                    "ALTER TABLE tools DROP COLUMN output_schema;
                     ALTER TABLE tools DROP COLUMN status;
                     DELETE FROM schema_version WHERE version = 2;",
                )?;
            }
        }
        conn.execute("INSERT OR IGNORE INTO schema_version(version) VALUES (3)", [])?;
        Ok(Self { conn })
    }

    fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolRecord> {
        Ok(ToolRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            language: parse_language(&row.get::<_, String>(3)?),
            entrypoint: row.get(4)?,
            input_schema: serde_json::from_str(&row.get::<_, String>(5)?)
                .unwrap_or(serde_json::Value::Null),
            capabilities: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
            keywords: parse_string_array(&row.get::<_, String>(7)?),
            commands: parse_string_array(&row.get::<_, String>(8)?),
            examples: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
            timeout_seconds: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
            version: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
            usage_count: row.get(14)?,
            last_used_at: row.get(15)?,
        })
    }

    pub fn insert_tool(&self, rec: &ToolRecord) -> Result<ToolRecord> {
        let caps = serde_json::to_string(&rec.capabilities).unwrap_or_else(|_| "{}".into());
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO tools (name, description, language, entrypoint, input_schema, capabilities, keywords, commands, examples, timeout_seconds, version, created_at, updated_at, usage_count, last_used_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                rec.name,
                rec.description,
                rec.language.as_str(),
                rec.entrypoint,
                rec.input_schema.to_string(),
                caps,
                serde_json::to_string(&rec.keywords).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&rec.commands).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&rec.examples).unwrap_or_else(|_| "[]".into()),
                rec.timeout_seconds.map(|v| v as i64),
                rec.version,
                rec.created_at,
                rec.updated_at,
                rec.usage_count,
                rec.last_used_at
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO tools_fts (rowid, name, description, keywords, commands) VALUES (?1,?2,?3,?4,?5)",
            params![id, rec.name, rec.description, rec.keywords.join("\n"), rec.commands.join("\n")],
        )?;
        tx.commit()?;
        let mut out = rec.clone();
        out.id = id;
        Ok(out)
    }

    pub fn get_tool(&self, name: &str) -> Result<Option<ToolRecord>> {
        let sql = format!("SELECT {} FROM tools WHERE name = ?1", TOOL_COLS);
        Ok(self.conn.query_row(&sql, [name], Self::row_to_record).optional()?)
    }

    pub fn list_tools(&self) -> Result<Vec<ToolRecord>> {
        let sql = format!("SELECT {} FROM tools ORDER BY name", TOOL_COLS);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::row_to_record)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn delete_tool(&self, name: &str) -> Result<bool> {
        match self.get_tool(name)? {
            Some(rec) => {
                let tx = self.conn.unchecked_transaction()?;
                tx.execute("DELETE FROM tools WHERE name = ?1", [name])?;
                tx.execute("DELETE FROM tools_fts WHERE rowid = ?1", [rec.id])?;
                tx.commit()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn update_tool(&self, rec: &ToolRecord) -> Result<ToolRecord> {
        let caps = serde_json::to_string(&rec.capabilities).unwrap_or_else(|_| "{}".into());
        let tx = self.conn.unchecked_transaction()?;
        let affected = tx.execute(
            "UPDATE tools SET description = ?2, language = ?3, entrypoint = ?4, input_schema = ?5,
             capabilities = ?6, keywords = ?7, commands = ?8, examples = ?9,
             timeout_seconds = ?10, version = ?11, updated_at = ?12
             WHERE name = ?1",
            params![
                rec.name,
                rec.description,
                rec.language.as_str(),
                rec.entrypoint,
                rec.input_schema.to_string(),
                caps,
                serde_json::to_string(&rec.keywords).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&rec.commands).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&rec.examples).unwrap_or_else(|_| "[]".into()),
                rec.timeout_seconds.map(|v| v as i64),
                rec.version,
                rec.updated_at
            ],
        )?;
        if affected == 0 {
            drop(tx);
            return Ok(rec.clone());
        }
        tx.execute("DELETE FROM tools_fts WHERE rowid = ?1", [rec.id])?;
        tx.execute(
            "INSERT INTO tools_fts (rowid, name, description, keywords, commands) VALUES (?1,?2,?3,?4,?5)",
            params![rec.id, rec.name, rec.description, rec.keywords.join("\n"), rec.commands.join("\n")],
        )?;
        tx.commit()?;
        Ok(rec.clone())
    }

    pub fn search(&self, match_query: &str, limit: usize) -> Result<Vec<(ToolRecord, f64)>> {
        let sql = format!(
            "SELECT t.{cols}, bm25(tools_fts) AS rank
             FROM tools_fts JOIN tools t ON t.id = tools_fts.rowid
             WHERE tools_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
            cols = TOOL_COLS.replace(", ", ", t.")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![match_query, limit as i64], |row| {
            Ok((Self::row_to_record(row)?, row.get::<_, f64>(16)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn like_search(&self, pattern: &str, limit: usize) -> Result<Vec<ToolRecord>> {
        let sql = format!(
            "SELECT {cols} FROM tools WHERE name LIKE ?1 ESCAPE '\\' OR description LIKE ?1 ESCAPE '\\' ORDER BY name LIMIT ?2",
            cols = TOOL_COLS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![pattern, limit as i64], Self::row_to_record)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn record_usage(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE tools SET usage_count = usage_count + 1, last_used_at = ?2 WHERE name = ?1",
            params![name, chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn fts5_available(&self) -> bool {
        self.conn
            .execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS temp._fts_probe USING fts5(x); DROP TABLE IF EXISTS temp._fts_probe;",
            )
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, Language};

    fn record(name: &str) -> ToolRecord {
        ToolRecord {
            id: 0,
            name: name.into(),
            description: format!("Tool for {}", name),
            language: Language::Bash,
            entrypoint: "run.sh".into(),
            input_schema: serde_json::json!({}),
            capabilities: Capabilities::default(),
            keywords: vec!["logs".into()],
            commands: vec!["jq".into()],
            examples: vec![serde_json::json!({"q": "err"})],
            timeout_seconds: Some(30),
            version: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            usage_count: 0,
            last_used_at: None,
        }
    }

    fn storage() -> (tempfile::TempDir, Storage) {
        let dir = tempfile::tempdir().unwrap();
        let s = Storage::open(&dir.path().join("repertoire.db")).unwrap();
        (dir, s)
    }

    #[test]
    fn open_is_idempotent_and_fts5_works() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("repertoire.db");
        let s = Storage::open(&db).unwrap();
        assert!(s.fts5_available());
        drop(s);
        assert!(Storage::open(&db).is_ok());
    }

    #[test]
    fn insert_get_list_delete() {
        let (_d, s) = storage();
        let inserted = s.insert_tool(&record("alpha_tool")).unwrap();
        assert!(inserted.id > 0);
        let got = s.get_tool("alpha_tool").unwrap().unwrap();
        assert_eq!(got.name, "alpha_tool");
        assert_eq!(got.language, Language::Bash);
        assert_eq!(got.keywords, vec!["logs".to_string()]);
        assert_eq!(got.commands, vec!["jq".to_string()]);
        assert_eq!(got.examples.len(), 1);
        assert_eq!(got.timeout_seconds, Some(30));
        assert_eq!(s.list_tools().unwrap().len(), 1);
        assert!(s.delete_tool("alpha_tool").unwrap());
        assert!(!s.delete_tool("alpha_tool").unwrap());
        assert!(s.get_tool("alpha_tool").unwrap().is_none());
    }

    #[test]
    fn rejects_v1_catalog_without_migrating() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("repertoire.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE UNIQUE INDEX schema_version_version_unique ON schema_version(version);
                 INSERT INTO schema_version(version) VALUES (1);
                 CREATE TABLE tools (
                     id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL,
                     language TEXT NOT NULL, entrypoint TEXT NOT NULL, input_schema TEXT NOT NULL,
                     output_schema TEXT NOT NULL DEFAULT '{}', capabilities TEXT NOT NULL DEFAULT '{}',
                     status TEXT NOT NULL DEFAULT 'published', version INTEGER NOT NULL DEFAULT 1,
                     created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                     usage_count INTEGER NOT NULL DEFAULT 0, last_used_at TEXT
                 );",
            )
            .unwrap();
        }
        let err = Storage::open(&db).unwrap_err();
        assert!(err.to_string().contains("incompatible"), "got: {}", err);
    }

    #[test]
    fn migrates_v2_catalog_dropping_dead_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("repertoire.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 CREATE UNIQUE INDEX schema_version_version_unique ON schema_version(version);
                 INSERT INTO schema_version(version) VALUES (2);
                 CREATE TABLE tools (
                     id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL,
                     language TEXT NOT NULL, entrypoint TEXT NOT NULL, input_schema TEXT NOT NULL,
                     output_schema TEXT NOT NULL DEFAULT '{}', capabilities TEXT NOT NULL DEFAULT '{}',
                     keywords TEXT NOT NULL DEFAULT '[]', commands TEXT NOT NULL DEFAULT '[]',
                     examples TEXT NOT NULL DEFAULT '[]', timeout_seconds INTEGER,
                     status TEXT NOT NULL DEFAULT 'published', version INTEGER NOT NULL DEFAULT 1,
                     created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
                     usage_count INTEGER NOT NULL DEFAULT 0, last_used_at TEXT
                 );
                 INSERT INTO tools (name, description, language, entrypoint, input_schema, keywords, created_at, updated_at)
                 VALUES ('legacy_tool', 'survives migration', 'bash', 'run.sh', '{}', '[\"legacy\"]', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
            )
            .unwrap();
        }
        let s = Storage::open(&db).unwrap();
        let got = s.get_tool("legacy_tool").unwrap().unwrap();
        assert_eq!(got.description, "survives migration");
        drop(s);

        let conn = rusqlite::Connection::open(&db).unwrap();
        let names: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(tools)").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert!(!names.iter().any(|c| c == "output_schema"), "columns: {names:?}");
        assert!(!names.iter().any(|c| c == "status"), "columns: {names:?}");
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
        drop(conn);

        assert!(Storage::open(&db).is_ok(), "second open is a no-op, not a re-migration");
    }

    #[test]
    fn fts_search_ranks_better_match_first() {
        let (_d, s) = storage();
        s.insert_tool(&record("gcp_errors")).unwrap();
        s.insert_tool(&record("gcp_deployments")).unwrap();
        let hits = s.search("\"gcp\"* OR \"errors\"*", 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0.name, "gcp_errors");
        assert!(hits[0].1 < hits[1].1);
    }

    #[test]
    fn like_search_finds_substring() {
        let (_d, s) = storage();
        s.insert_tool(&record("k8s_diag")).unwrap();
        let hits = s.like_search("%diag%", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "k8s_diag");
    }

    #[test]
    fn usage_recorded() {
        let (_d, s) = storage();
        s.insert_tool(&record("used_tool")).unwrap();
        s.record_usage("used_tool").unwrap();
        let got = s.get_tool("used_tool").unwrap().unwrap();
        assert_eq!(got.usage_count, 1);
        assert!(got.last_used_at.is_some());
    }

    #[test]
    fn update_tool_bumps_version_and_preserves_usage() {
        let (_d, s) = storage();
        s.insert_tool(&record("upd_tool")).unwrap();
        s.record_usage("upd_tool").unwrap();

        let mut rec = s.get_tool("upd_tool").unwrap().unwrap();
        rec.description = "new description".into();
        rec.keywords = vec!["new".into()];
        rec.commands = vec!["cat".into()];
        rec.version = 2;
        rec.updated_at = "2026-02-02T00:00:00Z".into();
        let out = s.update_tool(&rec).unwrap();
        assert_eq!(out.id, rec.id);

        let got = s.get_tool("upd_tool").unwrap().unwrap();
        assert_eq!(got.description, "new description");
        assert_eq!(got.version, 2);
        assert_eq!(got.usage_count, 1);
        assert!(got.last_used_at.is_some());
        assert_eq!(got.keywords, vec!["new".to_string()]);
        assert_eq!(got.commands, vec!["cat".to_string()]);

        let hits = s.search("\"new\"*", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.name, "upd_tool");
        let hits = s.search("\"old\"*", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn update_tool_missing_row_returns_zero_affected_without_error() {
        let (_d, s) = storage();
        let mut rec = record("ghost_upd");
        rec.id = 999;
        rec.version = 5;
        // The 0-rows guard returns before touching FTS, so no orphan FTS row is inserted.
        assert!(s.update_tool(&rec).is_ok());
        assert!(s.get_tool("ghost_upd").unwrap().is_none());
    }
}
