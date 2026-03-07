// Run:
// cargo run --example security_rotation_workflow -- <db-path> <registry-path>
//
// Seeds an encrypted tenant chunk and two tenant keys so rotation/verification
// workflows can be exercised against a file-backed database.

use serde_json::json;
use sqlrite::{
    AccessContext, AllowAllPolicy, ChunkInput, InMemoryTenantKeyRegistry, JsonlAuditLogger, Result,
    RuntimeConfig, SecureSqlRite, SqlRite, TenantKey,
};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let db_path = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("project_plan/reports/s28_rotation_demo.db"));
    let registry_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("project_plan/reports/s28_rotation_keys.json"));
    let audit_path = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("project_plan/reports/s28_rotation_audit.jsonl"));

    let db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;
    let audit = JsonlAuditLogger::new(audit_path, vec!["secret_payload".to_string()])?;
    let secure = SecureSqlRite::from_db(db, AllowAllPolicy, audit);

    let keys = InMemoryTenantKeyRegistry::load_from_json_file(&registry_path)?;
    keys.set_key("demo", TenantKey::new("k1", b"secret-key-00001")?, true)?;
    keys.set_key("demo", TenantKey::new("k2", b"secret-key-00002")?, false)?;
    keys.save_to_json_file(&registry_path)?;

    let actor = AccessContext::new("seed-user", "demo");
    secure.ingest_chunks_with_encryption(
        &actor,
        &[ChunkInput::new(
            "rotation-demo-1",
            "rotation-doc-1",
            "Rotation workflow demo content for tenant demo.",
            vec![1.0, 0.0, 0.0],
        )
        .with_metadata(json!({
            "tenant": "demo",
            "secret_payload": "rotatable-secret",
            "compliance_tag": "internal"
        }))],
        &keys,
        &["secret_payload"],
    )?;

    println!("seeded encrypted tenant chunk into {}", db_path.display());
    println!("registry saved to {}", registry_path.display());
    Ok(())
}
