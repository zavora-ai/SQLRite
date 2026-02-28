use crate::{Result, RuntimeConfig, SqlRite};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub schema_version: i64,
    pub chunk_count: usize,
    pub vector_index_mode: String,
    pub vector_index_storage_kind: String,
    pub vector_index_entries: usize,
    pub vector_index_estimated_memory_bytes: usize,
    pub integrity_check_ok: bool,
}

pub fn build_health_report(db: &SqlRite) -> Result<HealthReport> {
    let vector_stats = db.vector_index_stats();
    let vector_index_mode = vector_stats
        .as_ref()
        .map(|stats| stats.mode.clone())
        .unwrap_or_else(|| "disabled".to_string());
    let vector_index_entries = vector_stats
        .as_ref()
        .map(|stats| stats.entries)
        .unwrap_or(0);
    let vector_index_storage_kind = vector_stats
        .as_ref()
        .map(|stats| stats.storage_kind.clone())
        .unwrap_or_else(|| "f32".to_string());
    let vector_index_estimated_memory_bytes = vector_stats
        .as_ref()
        .map(|stats| stats.estimated_memory_bytes)
        .unwrap_or(0);

    Ok(HealthReport {
        schema_version: db.schema_version(),
        chunk_count: db.chunk_count()?,
        vector_index_mode,
        vector_index_storage_kind,
        vector_index_entries,
        vector_index_estimated_memory_bytes,
        integrity_check_ok: db.integrity_check_ok()?,
    })
}

pub fn backup_file(
    source_db_path: impl AsRef<Path>,
    backup_db_path: impl AsRef<Path>,
) -> Result<()> {
    let source_db_path = source_db_path.as_ref();
    let backup_db_path = backup_db_path.as_ref();

    let conn = Connection::open(source_db_path)?;
    conn.execute_batch("PRAGMA wal_checkpoint(FULL);")?;

    let backup_sql = format!(
        "VACUUM INTO {};",
        sqlite_quote_string(backup_db_path.to_string_lossy().as_ref())
    );
    conn.execute_batch(&backup_sql)?;
    Ok(())
}

pub fn verify_backup_file(path: impl AsRef<Path>) -> Result<HealthReport> {
    let db = SqlRite::open_with_config(path, RuntimeConfig::default())?;
    build_health_report(&db)
}

fn sqlite_quote_string(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn backup_and_verify_roundtrip() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("source.db");
        let backup = dir.path().join("backup.db");

        let db = SqlRite::open_with_config(&source, RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "hello backup".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;

        backup_file(&source, &backup)?;
        let report = verify_backup_file(&backup)?;
        assert!(report.integrity_check_ok);
        assert_eq!(report.chunk_count, 1);
        Ok(())
    }
}
