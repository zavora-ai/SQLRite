// Run: cargo run --example secure_tenant
// Demonstrates: tenant-scoped secure ingest/query with encrypted metadata fields.

use serde_json::json;
use sqlrite::{
    AccessContext, AllowAllPolicy, ChunkInput, InMemoryTenantKeyRegistry, JsonlAuditLogger, Result,
    RuntimeConfig, SearchRequest, SecureSqlRite, SqlRite, TenantKey,
};

fn main() -> Result<()> {
    let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
    let audit_path = std::env::temp_dir().join("sqlrite-example-audit.jsonl");
    let audit = JsonlAuditLogger::new(audit_path, vec!["secret_payload".to_string()])?;
    let secure = SecureSqlRite::from_db(db, AllowAllPolicy, audit);

    let keys = InMemoryTenantKeyRegistry::new();
    keys.set_active_key("acme", TenantKey::new("k1", b"example-key-material")?)?;

    let actor = AccessContext::new("user-1", "acme");
    secure.ingest_chunks_with_encryption(
        &actor,
        &[ChunkInput::new(
            "chunk-sec-1",
            "doc-sec-1",
            "Tenant data should be queryable only by that tenant.",
            vec![1.0, 0.0, 0.0],
        )
        .with_metadata(json!({
            "secret_payload": "sensitive-user-info"
        }))],
        &keys,
        &["secret_payload"],
    )?;

    let results = secure.search(&actor, SearchRequest::text("tenant data queryable", 5))?;

    println!("== secure_tenant results ==");
    println!("secure results: {}", results.len());
    if let Some(first) = results.first() {
        println!("top chunk: {}", first.chunk_id);
    }
    Ok(())
}
