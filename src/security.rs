use crate::{
    ChunkInput, Result, RuntimeConfig, SearchRequest, SearchResult, SqlRite, SqlRiteError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessOperation {
    Ingest,
    Query,
    DeleteTenant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessContext {
    pub actor_id: String,
    pub tenant_id: String,
    pub roles: Vec<String>,
}

impl AccessContext {
    pub fn new(actor_id: impl Into<String>, tenant_id: impl Into<String>) -> Self {
        Self {
            actor_id: actor_id.into(),
            tenant_id: tenant_id.into(),
            roles: Vec::new(),
        }
    }

    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    fn is_admin(&self) -> bool {
        self.roles.iter().any(|role| role == "admin")
    }
}

pub trait AccessPolicy: Send + Sync {
    fn authorize(
        &self,
        context: &AccessContext,
        operation: AccessOperation,
        target_tenant: &str,
    ) -> Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct AllowAllPolicy;

impl AccessPolicy for AllowAllPolicy {
    fn authorize(
        &self,
        context: &AccessContext,
        _operation: AccessOperation,
        target_tenant: &str,
    ) -> Result<()> {
        if context.tenant_id.trim().is_empty() || target_tenant.trim().is_empty() {
            return Err(SqlRiteError::InvalidTenantId);
        }
        if context.tenant_id != target_tenant && !context.is_admin() {
            return Err(SqlRiteError::AuthorizationDenied(format!(
                "tenant `{}` cannot access tenant `{}`",
                context.tenant_id, target_tenant
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantKey {
    pub key_id: String,
    pub material: Vec<u8>,
}

impl TenantKey {
    pub fn new(key_id: impl Into<String>, material: impl AsRef<[u8]>) -> Result<Self> {
        let key_id = key_id.into();
        let material = material.as_ref().to_vec();
        if key_id.trim().is_empty() || material.is_empty() {
            return Err(SqlRiteError::UnsupportedOperation(
                "tenant key_id/material are required".to_string(),
            ));
        }
        Ok(Self { key_id, material })
    }
}

pub trait TenantKeyRegistry: Send + Sync {
    fn active_key(&self, tenant_id: &str) -> Option<TenantKey>;
    fn key_by_id(&self, tenant_id: &str, key_id: &str) -> Option<TenantKey>;
}

#[derive(Debug, Default)]
pub struct InMemoryTenantKeyRegistry {
    keys: Mutex<HashMap<String, TenantKeyState>>,
}

#[derive(Debug, Default, Clone)]
struct TenantKeyState {
    active_key_id: Option<String>,
    keys: HashMap<String, TenantKey>,
}

impl InMemoryTenantKeyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_active_key(&self, tenant_id: &str, key: TenantKey) -> Result<()> {
        let mut guard = self.keys.lock().map_err(|_| {
            SqlRiteError::UnsupportedOperation("tenant key registry mutex poisoned".to_string())
        })?;
        let state = guard.entry(tenant_id.to_string()).or_default();
        state.active_key_id = Some(key.key_id.clone());
        state.keys.insert(key.key_id.clone(), key);
        Ok(())
    }

    pub fn set_key(&self, tenant_id: &str, key: TenantKey, make_active: bool) -> Result<()> {
        let mut guard = self.keys.lock().map_err(|_| {
            SqlRiteError::UnsupportedOperation("tenant key registry mutex poisoned".to_string())
        })?;
        let state = guard.entry(tenant_id.to_string()).or_default();
        if make_active {
            state.active_key_id = Some(key.key_id.clone());
        }
        state.keys.insert(key.key_id.clone(), key);
        Ok(())
    }

    pub fn load_from_json_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }
        let payload = fs::read_to_string(path)?;
        let serializable = serde_json::from_str::<SerializableTenantKeyRegistry>(&payload)
            .map_err(|e| SqlRiteError::UnsupportedOperation(e.to_string()))?;

        let mut tenants = HashMap::new();
        for (tenant_id, state) in serializable.tenants {
            let mut keys = HashMap::new();
            for key in state.keys {
                keys.insert(key.key_id.clone(), key);
            }
            tenants.insert(
                tenant_id,
                TenantKeyState {
                    active_key_id: state.active_key_id,
                    keys,
                },
            );
        }
        Ok(Self {
            keys: Mutex::new(tenants),
        })
    }

    pub fn save_to_json_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }

        let guard = self.keys.lock().map_err(|_| {
            SqlRiteError::UnsupportedOperation("tenant key registry mutex poisoned".to_string())
        })?;
        let tenants = guard
            .iter()
            .map(|(tenant_id, state)| {
                let keys = state.keys.values().cloned().collect::<Vec<_>>();
                (
                    tenant_id.clone(),
                    SerializableTenantKeyState {
                        active_key_id: state.active_key_id.clone(),
                        keys,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let payload = serde_json::to_string_pretty(&SerializableTenantKeyRegistry { tenants })?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, payload)?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableTenantKeyState {
    active_key_id: Option<String>,
    keys: Vec<TenantKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableTenantKeyRegistry {
    tenants: HashMap<String, SerializableTenantKeyState>,
}

impl TenantKeyRegistry for InMemoryTenantKeyRegistry {
    fn active_key(&self, tenant_id: &str) -> Option<TenantKey> {
        let guard = self.keys.lock().ok()?;
        let state = guard.get(tenant_id)?;
        let active = state.active_key_id.as_ref()?;
        state.keys.get(active).cloned()
    }

    fn key_by_id(&self, tenant_id: &str, key_id: &str) -> Option<TenantKey> {
        let guard = self.keys.lock().ok()?;
        guard.get(tenant_id)?.keys.get(key_id).cloned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub unix_ms: u64,
    pub actor_id: String,
    pub tenant_id: String,
    pub operation: AccessOperation,
    pub allowed: bool,
    pub detail: Value,
}

pub trait AuditLogger: Send + Sync {
    fn log(&self, event: &AuditEvent) -> Result<()>;
}

#[derive(Debug)]
pub struct JsonlAuditLogger {
    path: PathBuf,
    redacted_fields: HashSet<String>,
    lock: Mutex<()>,
}

impl JsonlAuditLogger {
    pub fn new(
        path: impl AsRef<Path>,
        redacted_fields: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            path,
            redacted_fields: redacted_fields.into_iter().collect(),
            lock: Mutex::new(()),
        })
    }

    fn redact(&self, detail: &Value) -> Value {
        match detail {
            Value::Object(map) => {
                let mut copy = map.clone();
                for key in &self.redacted_fields {
                    if copy.contains_key(key) {
                        copy.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                    }
                }
                Value::Object(copy)
            }
            _ => detail.clone(),
        }
    }
}

impl AuditLogger for JsonlAuditLogger {
    fn log(&self, event: &AuditEvent) -> Result<()> {
        let _guard = self.lock.lock().map_err(|_| {
            SqlRiteError::UnsupportedOperation("audit logger mutex poisoned".to_string())
        })?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let serialized = serde_json::to_string(&AuditEvent {
            detail: self.redact(&event.detail),
            ..event.clone()
        })?;
        file.write_all(serialized.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

pub struct SecureSqlRite<P: AccessPolicy, A: AuditLogger> {
    db: SqlRite,
    policy: P,
    audit_logger: A,
}

impl<P: AccessPolicy, A: AuditLogger> SecureSqlRite<P, A> {
    pub fn open_with_config(
        path: impl AsRef<Path>,
        runtime: RuntimeConfig,
        policy: P,
        audit_logger: A,
    ) -> Result<Self> {
        Ok(Self {
            db: SqlRite::open_with_config(path, runtime)?,
            policy,
            audit_logger,
        })
    }

    pub fn from_db(db: SqlRite, policy: P, audit_logger: A) -> Self {
        Self {
            db,
            policy,
            audit_logger,
        }
    }

    pub fn ingest_chunks(&self, context: &AccessContext, chunks: &[ChunkInput]) -> Result<()> {
        self.policy
            .authorize(context, AccessOperation::Ingest, &context.tenant_id)?;

        let mut enriched = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let metadata = merge_tenant_metadata(&chunk.metadata, &context.tenant_id);
            enriched.push(ChunkInput {
                id: chunk.id.clone(),
                doc_id: chunk.doc_id.clone(),
                content: chunk.content.clone(),
                embedding: chunk.embedding.clone(),
                metadata,
                source: chunk.source.clone(),
            });
        }

        let result = self.db.ingest_chunks(&enriched);
        self.audit(
            context,
            AccessOperation::Ingest,
            result.is_ok(),
            serde_json::json!({
                "chunk_count": chunks.len(),
            }),
        )?;
        result
    }

    pub fn ingest_chunks_with_encryption<R: TenantKeyRegistry>(
        &self,
        context: &AccessContext,
        chunks: &[ChunkInput],
        key_registry: &R,
        sensitive_metadata_fields: &[&str],
    ) -> Result<()> {
        self.policy
            .authorize(context, AccessOperation::Ingest, &context.tenant_id)?;
        let active_key = key_registry.active_key(&context.tenant_id).ok_or_else(|| {
            SqlRiteError::UnsupportedOperation(format!(
                "no active key configured for tenant `{}`",
                context.tenant_id
            ))
        })?;

        let mut encrypted_chunks = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let mut metadata = merge_tenant_metadata(&chunk.metadata, &context.tenant_id);
            encrypt_metadata_fields(
                &mut metadata,
                &active_key,
                &context.tenant_id,
                sensitive_metadata_fields,
            )?;

            encrypted_chunks.push(ChunkInput {
                id: chunk.id.clone(),
                doc_id: chunk.doc_id.clone(),
                content: chunk.content.clone(),
                embedding: chunk.embedding.clone(),
                metadata,
                source: chunk.source.clone(),
            });
        }

        self.db.ingest_chunks(&encrypted_chunks)
    }

    pub fn search(
        &self,
        context: &AccessContext,
        mut request: SearchRequest,
    ) -> Result<Vec<SearchResult>> {
        self.policy
            .authorize(context, AccessOperation::Query, &context.tenant_id)?;

        if let Some(existing) = request.metadata_filters.get("tenant")
            && existing != &context.tenant_id
        {
            self.audit(
                context,
                AccessOperation::Query,
                false,
                serde_json::json!({"reason": "tenant filter mismatch"}),
            )?;
            return Err(SqlRiteError::AuthorizationDenied(
                "tenant filter mismatch".to_string(),
            ));
        }

        request
            .metadata_filters
            .insert("tenant".to_string(), context.tenant_id.clone());
        let result = self.db.search(request);

        self.audit(
            context,
            AccessOperation::Query,
            result.is_ok(),
            serde_json::json!({
                "result_count": result.as_ref().map(|items| items.len()).unwrap_or(0),
            }),
        )?;
        result
    }

    pub fn delete_tenant_data(&self, context: &AccessContext, tenant_id: &str) -> Result<usize> {
        self.policy
            .authorize(context, AccessOperation::DeleteTenant, tenant_id)?;

        let result = self.db.delete_chunks_by_metadata("tenant", tenant_id);
        self.audit(
            context,
            AccessOperation::DeleteTenant,
            result.is_ok(),
            serde_json::json!({
                "target_tenant": tenant_id,
                "deleted": result.as_ref().copied().unwrap_or(0),
            }),
        )?;
        result
    }

    pub fn db(&self) -> &SqlRite {
        &self.db
    }

    pub fn into_inner(self) -> SqlRite {
        self.db
    }

    fn audit(
        &self,
        context: &AccessContext,
        operation: AccessOperation,
        allowed: bool,
        detail: Value,
    ) -> Result<()> {
        self.audit_logger.log(&AuditEvent {
            unix_ms: now_unix_ms(),
            actor_id: context.actor_id.clone(),
            tenant_id: context.tenant_id.clone(),
            operation,
            allowed,
            detail,
        })
    }
}

pub fn rotate_tenant_encryption_key<R: TenantKeyRegistry>(
    db: &SqlRite,
    tenant_id: &str,
    metadata_field: &str,
    key_registry: &R,
    new_key_id: &str,
) -> Result<usize> {
    let new_key = key_registry
        .key_by_id(tenant_id, new_key_id)
        .ok_or_else(|| SqlRiteError::UnsupportedOperation("new key not found".to_string()))?;

    let mut updated = 0usize;
    let mut offset = 0usize;
    const PAGE_SIZE: usize = 256;

    loop {
        let page = db.list_chunks_page(offset, PAGE_SIZE, Some(tenant_id))?;
        if page.is_empty() {
            break;
        }

        for chunk in &page {
            let mut metadata = chunk.metadata.clone();
            let Some(encrypted_value) = metadata.get(metadata_field).and_then(Value::as_str) else {
                continue;
            };
            let Some((old_key_id, cipher_hex)) = parse_encrypted_value(encrypted_value) else {
                continue;
            };

            let old_key = key_registry
                .key_by_id(tenant_id, old_key_id)
                .ok_or_else(|| {
                    SqlRiteError::UnsupportedOperation(format!(
                        "old key `{old_key_id}` not found for tenant `{tenant_id}`"
                    ))
                })?;

            let plaintext = decrypt_with_key(cipher_hex, &old_key.material)?;
            let rotated = encrypt_with_key(&plaintext, tenant_id, &new_key);
            if let Value::Object(ref mut map) = metadata {
                map.insert(metadata_field.to_string(), Value::String(rotated));
                map.insert(
                    "tenant_key_id".to_string(),
                    Value::String(new_key.key_id.clone()),
                );
            }

            db.update_chunk_metadata(&chunk.id, &metadata)?;
            updated += 1;
        }

        offset += page.len();
    }

    Ok(updated)
}

fn merge_tenant_metadata(metadata: &Value, tenant_id: &str) -> Value {
    match metadata {
        Value::Object(map) => {
            let mut merged = map.clone();
            merged.insert("tenant".to_string(), Value::String(tenant_id.to_string()));
            Value::Object(merged)
        }
        _ => {
            let mut merged = serde_json::Map::new();
            merged.insert("tenant".to_string(), Value::String(tenant_id.to_string()));
            Value::Object(merged)
        }
    }
}

fn encrypt_metadata_fields(
    metadata: &mut Value,
    key: &TenantKey,
    tenant_id: &str,
    sensitive_metadata_fields: &[&str],
) -> Result<()> {
    let Some(map) = metadata.as_object_mut() else {
        return Err(SqlRiteError::UnsupportedOperation(
            "metadata must be a json object for encryption".to_string(),
        ));
    };

    for field in sensitive_metadata_fields {
        let Some(raw) = map.get(*field).and_then(Value::as_str) else {
            continue;
        };
        map.insert(
            (*field).to_string(),
            Value::String(encrypt_with_key(raw, tenant_id, key)),
        );
    }
    map.insert(
        "tenant_key_id".to_string(),
        Value::String(key.key_id.clone()),
    );
    Ok(())
}

fn encrypt_with_key(plaintext: &str, tenant_id: &str, key: &TenantKey) -> String {
    let mut scoped = Vec::new();
    scoped.extend_from_slice(tenant_id.as_bytes());
    scoped.push(0);
    scoped.extend_from_slice(plaintext.as_bytes());
    let cipher = xor_with_key(&scoped, &key.material);
    format!("enc:v1:{}:{}", key.key_id, hex_encode(&cipher))
}

fn decrypt_with_key(cipher_hex: &str, key_material: &[u8]) -> Result<String> {
    let cipher = hex_decode(cipher_hex)?;
    let plain_scoped = xor_with_key(&cipher, key_material);
    let Some(separator_idx) = plain_scoped.iter().position(|byte| *byte == 0) else {
        return Err(SqlRiteError::UnsupportedOperation(
            "invalid encrypted payload format".to_string(),
        ));
    };
    let plaintext = &plain_scoped[(separator_idx + 1)..];
    String::from_utf8(plaintext.to_vec()).map_err(|_| {
        SqlRiteError::UnsupportedOperation("invalid utf8 in decrypted payload".to_string())
    })
}

fn xor_with_key(input: &[u8], key: &[u8]) -> Vec<u8> {
    input
        .iter()
        .enumerate()
        .map(|(idx, byte)| byte ^ key[idx % key.len()])
        .collect()
}

fn parse_encrypted_value(value: &str) -> Option<(&str, &str)> {
    let mut parts = value.splitn(4, ':');
    let marker = parts.next()?;
    let version = parts.next()?;
    let key_id = parts.next()?;
    let payload = parts.next()?;
    if marker == "enc" && version == "v1" && !key_id.is_empty() && !payload.is_empty() {
        Some((key_id, payload))
    } else {
        None
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(value: &str) -> Result<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return Err(SqlRiteError::UnsupportedOperation(
            "invalid hex payload length".to_string(),
        ));
    }
    let mut out = Vec::with_capacity(value.len() / 2);
    for idx in (0..value.len()).step_by(2) {
        let byte = u8::from_str_radix(&value[idx..idx + 2], 16)
            .map_err(|_| SqlRiteError::UnsupportedOperation("invalid hex payload".to_string()))?;
        out.push(byte);
    }
    Ok(out)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig, SearchRequest, SqlRite};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn secure_wrapper_enforces_tenant_filter() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let tmp = tempdir()?;
        let logger = JsonlAuditLogger::new(tmp.path().join("audit.jsonl"), Vec::<String>::new())?;
        let secure = SecureSqlRite::from_db(db, AllowAllPolicy, logger);

        let ctx_acme = AccessContext::new("user-1", "acme");
        secure.ingest_chunks(
            &ctx_acme,
            &[ChunkInput {
                id: "c1".to_string(),
                doc_id: "d1".to_string(),
                content: "tenant scoped".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({}),
                source: None,
            }],
        )?;

        let ctx_beta = AccessContext::new("user-2", "beta");
        let beta_results = secure.search(
            &ctx_beta,
            SearchRequest {
                query_text: Some("tenant".to_string()),
                top_k: 5,
                ..Default::default()
            },
        )?;
        assert!(beta_results.is_empty());

        let acme_results = secure.search(
            &ctx_acme,
            SearchRequest {
                query_text: Some("tenant".to_string()),
                top_k: 5,
                ..Default::default()
            },
        )?;
        assert_eq!(acme_results.len(), 1);
        Ok(())
    }

    #[test]
    fn non_admin_cannot_delete_other_tenant() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let tmp = tempdir()?;
        let logger = JsonlAuditLogger::new(tmp.path().join("audit.jsonl"), Vec::<String>::new())?;
        let secure = SecureSqlRite::from_db(db, AllowAllPolicy, logger);

        let err = secure
            .delete_tenant_data(&AccessContext::new("u1", "acme"), "beta")
            .expect_err("cross tenant delete should fail");
        assert!(matches!(err, SqlRiteError::AuthorizationDenied(_)));
        Ok(())
    }

    #[test]
    fn encrypted_ingest_and_key_rotation_workflow() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let tmp = tempdir()?;
        let logger = JsonlAuditLogger::new(tmp.path().join("audit.jsonl"), Vec::<String>::new())?;
        let secure = SecureSqlRite::from_db(db, AllowAllPolicy, logger);

        let key_registry = InMemoryTenantKeyRegistry::new();
        key_registry.set_active_key("acme", TenantKey::new("k1", b"secret-key-1")?)?;
        key_registry.set_active_key("acme", TenantKey::new("k2", b"secret-key-2")?)?;

        let ctx = AccessContext::new("user-enc", "acme");
        secure.ingest_chunks_with_encryption(
            &ctx,
            &[ChunkInput {
                id: "c-sec".to_string(),
                doc_id: "d-sec".to_string(),
                content: "sensitive chunk".to_string(),
                embedding: vec![1.0, 0.0],
                metadata: json!({"secret_payload": "highly-sensitive"}),
                source: None,
            }],
            &key_registry,
            &["secret_payload"],
        )?;

        let before = secure
            .db()
            .list_chunks_page(0, 10, Some("acme"))?
            .into_iter()
            .next()
            .expect("chunk exists");
        let before_payload = before
            .metadata
            .get("secret_payload")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        assert!(before_payload.starts_with("enc:v1:"));

        let rotated = rotate_tenant_encryption_key(
            secure.db(),
            "acme",
            "secret_payload",
            &key_registry,
            "k1",
        )?;
        assert_eq!(rotated, 1);

        let after = secure
            .db()
            .list_chunks_page(0, 10, Some("acme"))?
            .into_iter()
            .next()
            .expect("chunk exists");
        let after_payload = after
            .metadata
            .get("secret_payload")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        assert!(after_payload.starts_with("enc:v1:k1:"));
        Ok(())
    }

    #[test]
    fn key_registry_persists_to_disk() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("tenant_keys.json");
        let registry = InMemoryTenantKeyRegistry::new();
        registry.set_active_key("acme", TenantKey::new("k1", b"material-1")?)?;
        registry.set_key("acme", TenantKey::new("k2", b"material-2")?, false)?;
        registry.save_to_json_file(&path)?;

        let restored = InMemoryTenantKeyRegistry::load_from_json_file(&path)?;
        assert!(restored.active_key("acme").is_some());
        assert!(restored.key_by_id("acme", "k2").is_some());
        Ok(())
    }
}
