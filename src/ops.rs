use crate::{Result, RuntimeConfig, SqlRite};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSnapshotRecord {
    pub snapshot_id: String,
    pub source_db_path: String,
    pub snapshot_path: String,
    pub created_unix_ms: u64,
    pub size_bytes: u64,
    pub note: Option<String>,
    pub integrity_ok: Option<bool>,
    pub chunk_count: Option<usize>,
    pub schema_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPruneReport {
    pub retention_seconds: u64,
    pub cutoff_unix_ms: u64,
    pub kept_count: usize,
    pub removed_count: usize,
    pub removed_paths: Vec<String>,
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

pub fn create_backup_snapshot(
    source_db_path: impl AsRef<Path>,
    backup_dir: impl AsRef<Path>,
    note: Option<&str>,
) -> Result<BackupSnapshotRecord> {
    let source_db_path = source_db_path.as_ref();
    let backup_dir = backup_dir.as_ref();
    let snapshots_dir = snapshots_dir(backup_dir);
    fs::create_dir_all(&snapshots_dir)?;

    let created_unix_ms = unix_ms_now();
    let snapshot_id = format!("snap-{created_unix_ms}");
    let file_name = build_snapshot_file_name(&snapshot_id, note);
    let snapshot_path = snapshots_dir.join(file_name);

    backup_file(source_db_path, &snapshot_path)?;
    let verification = verify_backup_file(&snapshot_path)?;
    let size_bytes = fs::metadata(&snapshot_path)?.len();

    let record = BackupSnapshotRecord {
        snapshot_id,
        source_db_path: source_db_path.display().to_string(),
        snapshot_path: snapshot_path.display().to_string(),
        created_unix_ms,
        size_bytes,
        note: note.map(str::to_string),
        integrity_ok: Some(verification.integrity_check_ok),
        chunk_count: Some(verification.chunk_count),
        schema_version: Some(verification.schema_version),
    };
    append_snapshot_record(backup_dir, &record)?;
    Ok(record)
}

pub fn list_backup_snapshots(backup_dir: impl AsRef<Path>) -> Result<Vec<BackupSnapshotRecord>> {
    let backup_dir = backup_dir.as_ref();
    let mut records = read_snapshot_catalog(backup_dir)?;
    if records.is_empty() {
        records = discover_snapshots_without_catalog(backup_dir)?;
    }

    let mut seen_paths = HashSet::new();
    records.retain(|record| seen_paths.insert(record.snapshot_path.clone()));
    records.sort_by(|a, b| b.created_unix_ms.cmp(&a.created_unix_ms));
    Ok(records)
}

pub fn select_backup_snapshot_for_time(
    backup_dir: impl AsRef<Path>,
    target_unix_ms: u64,
) -> Result<Option<BackupSnapshotRecord>> {
    let snapshots = list_backup_snapshots(backup_dir)?;
    Ok(snapshots
        .into_iter()
        .filter(|record| record.created_unix_ms <= target_unix_ms)
        .filter(|record| Path::new(&record.snapshot_path).exists())
        .max_by_key(|record| record.created_unix_ms))
}

pub fn restore_backup_file(
    source_backup_path: impl AsRef<Path>,
    destination_db_path: impl AsRef<Path>,
) -> Result<()> {
    let source_backup_path = source_backup_path.as_ref();
    let destination_db_path = destination_db_path.as_ref();

    if let Some(parent) = destination_db_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    remove_db_with_sidecars(destination_db_path)?;
    let conn = Connection::open(source_backup_path)?;
    conn.execute_batch("PRAGMA wal_checkpoint(FULL);")?;
    let restore_sql = format!(
        "VACUUM INTO {};",
        sqlite_quote_string(destination_db_path.to_string_lossy().as_ref())
    );
    conn.execute_batch(&restore_sql)?;
    Ok(())
}

pub fn restore_backup_file_verified(
    source_backup_path: impl AsRef<Path>,
    destination_db_path: impl AsRef<Path>,
) -> Result<HealthReport> {
    restore_backup_file(source_backup_path, &destination_db_path)?;
    verify_backup_file(destination_db_path)
}

pub fn prune_backup_snapshots(
    backup_dir: impl AsRef<Path>,
    retention_seconds: u64,
) -> Result<BackupPruneReport> {
    prune_backup_snapshots_at(backup_dir.as_ref(), retention_seconds, unix_ms_now())
}

fn prune_backup_snapshots_at(
    backup_dir: &Path,
    retention_seconds: u64,
    now_unix_ms: u64,
) -> Result<BackupPruneReport> {
    let mut snapshots = list_backup_snapshots(backup_dir)?;
    snapshots.sort_by(|a, b| b.created_unix_ms.cmp(&a.created_unix_ms));
    let cutoff_unix_ms = now_unix_ms.saturating_sub(retention_seconds.saturating_mul(1_000));
    let mut kept_count = 0usize;
    let mut removed_count = 0usize;
    let mut removed_paths = Vec::new();

    for (idx, snapshot) in snapshots.iter().enumerate() {
        let path = Path::new(&snapshot.snapshot_path);
        if !path.exists() {
            continue;
        }
        if idx == 0 || snapshot.created_unix_ms >= cutoff_unix_ms {
            kept_count = kept_count.saturating_add(1);
            continue;
        }
        fs::remove_file(path)?;
        removed_count = removed_count.saturating_add(1);
        removed_paths.push(snapshot.snapshot_path.clone());
    }

    Ok(BackupPruneReport {
        retention_seconds,
        cutoff_unix_ms,
        kept_count,
        removed_count,
        removed_paths,
    })
}

fn append_snapshot_record(backup_dir: &Path, record: &BackupSnapshotRecord) -> Result<()> {
    fs::create_dir_all(backup_dir)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(catalog_path(backup_dir))?;
    let line = serde_json::to_string(record)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn read_snapshot_catalog(backup_dir: &Path) -> Result<Vec<BackupSnapshotRecord>> {
    let path = catalog_path(backup_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<BackupSnapshotRecord>(trimmed).map_err(|error| {
            std::io::Error::other(format!(
                "invalid backup catalog entry on line {}: {error}",
                idx + 1
            ))
        })?;
        records.push(record);
    }
    Ok(records)
}

fn discover_snapshots_without_catalog(backup_dir: &Path) -> Result<Vec<BackupSnapshotRecord>> {
    let snapshot_root = snapshots_dir(backup_dir);
    if !snapshot_root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(&snapshot_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("db") {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let created_unix_ms = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis() as u64;
        let snapshot_path = path.display().to_string();
        let stem = path
            .file_stem()
            .and_then(|raw| raw.to_str())
            .unwrap_or("snapshot");
        records.push(BackupSnapshotRecord {
            snapshot_id: stem.to_string(),
            source_db_path: "<unknown>".to_string(),
            snapshot_path,
            created_unix_ms,
            size_bytes: metadata.len(),
            note: None,
            integrity_ok: None,
            chunk_count: None,
            schema_version: None,
        });
    }
    Ok(records)
}

fn catalog_path(backup_dir: &Path) -> PathBuf {
    backup_dir.join("backup_catalog.jsonl")
}

fn snapshots_dir(backup_dir: &Path) -> PathBuf {
    backup_dir.join("snapshots")
}

fn build_snapshot_file_name(snapshot_id: &str, note: Option<&str>) -> String {
    if let Some(note) = note {
        let slug = sanitize_snapshot_note(note);
        if !slug.is_empty() {
            return format!("{snapshot_id}-{slug}.db");
        }
    }
    format!("{snapshot_id}.db")
}

fn sanitize_snapshot_note(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn remove_db_with_sidecars(db_path: &Path) -> Result<()> {
    let sidecars = ["", "-wal", "-shm"];
    for suffix in sidecars {
        let path = if suffix.is_empty() {
            db_path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", db_path.display(), suffix))
        };
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
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
    use std::thread::sleep;
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

    #[test]
    fn snapshot_catalog_and_pitr_selection_roundtrip() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("source.db");
        let backup_dir = dir.path().join("backups");
        let restore_target = dir.path().join("restore.db");

        let db = SqlRite::open_with_config(&source, RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "before snapshot".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        let snap_one = create_backup_snapshot(&source, &backup_dir, Some("before"))?;
        sleep(Duration::from_millis(2));

        db.ingest_chunk(&ChunkInput {
            id: "c2".to_string(),
            doc_id: "d1".to_string(),
            content: "after snapshot".to_string(),
            embedding: vec![0.0, 1.0],
            metadata: json!({}),
            source: None,
        })?;
        let snap_two = create_backup_snapshot(&source, &backup_dir, Some("after"))?;

        let snapshots = list_backup_snapshots(&backup_dir)?;
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].snapshot_id, snap_two.snapshot_id);

        let selected = select_backup_snapshot_for_time(&backup_dir, snap_one.created_unix_ms + 1)?
            .expect("expected snapshot selection");
        assert_eq!(selected.snapshot_id, snap_one.snapshot_id);

        let restored = restore_backup_file_verified(&selected.snapshot_path, &restore_target)?;
        assert!(restored.integrity_check_ok);
        assert_eq!(restored.chunk_count, 1);
        Ok(())
    }

    #[test]
    fn restore_backup_overwrites_existing_destination() -> Result<()> {
        let dir = tempdir()?;
        let source = dir.path().join("source.db");
        let backup = dir.path().join("backup.db");
        let destination = dir.path().join("destination.db");

        let source_db = SqlRite::open_with_config(&source, RuntimeConfig::default())?;
        source_db.ingest_chunk(&ChunkInput {
            id: "a1".to_string(),
            doc_id: "d1".to_string(),
            content: "source payload".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        source_db.ingest_chunk(&ChunkInput {
            id: "a2".to_string(),
            doc_id: "d1".to_string(),
            content: "source payload 2".to_string(),
            embedding: vec![0.0, 1.0],
            metadata: json!({}),
            source: None,
        })?;
        backup_file(&source, &backup)?;

        let dest_db = SqlRite::open_with_config(&destination, RuntimeConfig::default())?;
        dest_db.ingest_chunk(&ChunkInput {
            id: "b1".to_string(),
            doc_id: "x".to_string(),
            content: "stale destination".to_string(),
            embedding: vec![0.5, 0.5],
            metadata: json!({}),
            source: None,
        })?;
        drop(dest_db);

        let restored = restore_backup_file_verified(&backup, &destination)?;
        assert!(restored.integrity_check_ok);
        assert_eq!(restored.chunk_count, 2);
        Ok(())
    }

    #[test]
    fn prune_snapshots_removes_old_entries_and_keeps_latest() -> Result<()> {
        let dir = tempdir()?;
        let backup_dir = dir.path().join("backups");
        let snapshots = snapshots_dir(&backup_dir);
        fs::create_dir_all(&snapshots)?;

        let old_path = snapshots.join("snap-1000-old.db");
        let middle_path = snapshots.join("snap-2000-mid.db");
        let latest_path = snapshots.join("snap-3000-new.db");
        fs::write(&old_path, "old")?;
        fs::write(&middle_path, "mid")?;
        fs::write(&latest_path, "new")?;

        append_snapshot_record(
            &backup_dir,
            &BackupSnapshotRecord {
                snapshot_id: "snap-1000".to_string(),
                source_db_path: "source.db".to_string(),
                snapshot_path: old_path.display().to_string(),
                created_unix_ms: 1_000,
                size_bytes: 3,
                note: None,
                integrity_ok: None,
                chunk_count: None,
                schema_version: None,
            },
        )?;
        append_snapshot_record(
            &backup_dir,
            &BackupSnapshotRecord {
                snapshot_id: "snap-2000".to_string(),
                source_db_path: "source.db".to_string(),
                snapshot_path: middle_path.display().to_string(),
                created_unix_ms: 2_000,
                size_bytes: 3,
                note: None,
                integrity_ok: None,
                chunk_count: None,
                schema_version: None,
            },
        )?;
        append_snapshot_record(
            &backup_dir,
            &BackupSnapshotRecord {
                snapshot_id: "snap-3000".to_string(),
                source_db_path: "source.db".to_string(),
                snapshot_path: latest_path.display().to_string(),
                created_unix_ms: 3_000,
                size_bytes: 3,
                note: None,
                integrity_ok: None,
                chunk_count: None,
                schema_version: None,
            },
        )?;

        let report = prune_backup_snapshots_at(&backup_dir, 5, 10_000)?;
        assert_eq!(report.removed_count, 2);
        assert_eq!(report.kept_count, 1);
        assert!(!old_path.exists());
        assert!(!middle_path.exists());
        assert!(latest_path.exists());
        Ok(())
    }
}
