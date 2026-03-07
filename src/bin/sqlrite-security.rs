use sqlrite::{
    AuditExportFormat, AuditQuery, InMemoryTenantKeyRegistry, RbacPolicy, RuntimeConfig, SqlRite,
    TenantKey, export_audit_events, inspect_tenant_key_rotation,
    rotate_tenant_encryption_key_with_report,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args
        .first()
        .cloned()
        .ok_or_else(|| std::io::Error::other(usage()))?;

    match command.as_str() {
        "init-policy" => {
            let path = PathBuf::from(arg_value(&args, "--path")?);
            let policy = RbacPolicy::default();
            policy.save_to_json_file(&path)?;
            println!("rbac policy written to {}", path.display());
        }
        "add-key" => {
            let registry_path = PathBuf::from(arg_value(&args, "--registry")?);
            let tenant = arg_value(&args, "--tenant")?;
            let key_id = arg_value(&args, "--key-id")?;
            let key_material = arg_value(&args, "--key-material")?;
            let active = has_flag(&args, "--active");

            let registry = InMemoryTenantKeyRegistry::load_from_json_file(&registry_path)?;
            registry.set_key(
                &tenant,
                TenantKey::new(key_id, key_material.as_bytes())?,
                active,
            )?;
            registry.save_to_json_file(&registry_path)?;
            println!("tenant key added");
        }
        "rotate-key" => {
            let db_path = PathBuf::from(arg_value(&args, "--db")?);
            let registry_path = PathBuf::from(arg_value(&args, "--registry")?);
            let tenant = arg_value(&args, "--tenant")?;
            let metadata_field = arg_value(&args, "--field")?;
            let new_key_id = arg_value(&args, "--new-key-id")?;
            let json = has_flag(&args, "--json");

            let db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;
            let registry = InMemoryTenantKeyRegistry::load_from_json_file(&registry_path)?;
            let report = rotate_tenant_encryption_key_with_report(
                &db,
                &tenant,
                &metadata_field,
                &registry,
                &new_key_id,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "rotated encrypted metadata for {} chunk(s); verified_all_target_key={}",
                    report.rotated_chunks, report.verified_all_target_key
                );
            }
        }
        "verify-key" => {
            let db_path = PathBuf::from(arg_value(&args, "--db")?);
            let registry_path = PathBuf::from(arg_value(&args, "--registry")?);
            let tenant = arg_value(&args, "--tenant")?;
            let metadata_field = arg_value(&args, "--field")?;
            let key_id = arg_value(&args, "--key-id")?;

            let db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;
            let registry = InMemoryTenantKeyRegistry::load_from_json_file(&registry_path)?;
            let report =
                inspect_tenant_key_rotation(&db, &tenant, &metadata_field, &registry, &key_id)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        "export-audit" => {
            let input_path = PathBuf::from(arg_value(&args, "--input")?);
            let output_path = PathBuf::from(arg_value(&args, "--output")?);
            let format = match optional_arg_value(&args, "--format").as_deref() {
                Some("json") => AuditExportFormat::Json,
                Some("jsonl") | None => AuditExportFormat::Jsonl,
                Some(other) => {
                    return Err(std::io::Error::other(format!(
                        "invalid --format `{other}`\n{}",
                        usage()
                    ))
                    .into());
                }
            };
            let query = AuditQuery {
                actor_id: optional_arg_value(&args, "--actor"),
                tenant_id: optional_arg_value(&args, "--tenant"),
                operation: optional_arg_value(&args, "--operation")
                    .map(|value| parse_operation(&value))
                    .transpose()
                    .map_err(std::io::Error::other)?,
                allowed: optional_arg_value(&args, "--allowed")
                    .map(|value| parse_bool(&value))
                    .transpose()
                    .map_err(std::io::Error::other)?,
                from_unix_ms: optional_arg_value(&args, "--from-ms")
                    .map(|value| parse_u64_flag(&value, "--from-ms"))
                    .transpose()
                    .map_err(std::io::Error::other)?,
                to_unix_ms: optional_arg_value(&args, "--to-ms")
                    .map(|value| parse_u64_flag(&value, "--to-ms"))
                    .transpose()
                    .map_err(std::io::Error::other)?,
                limit: optional_arg_value(&args, "--limit")
                    .map(|value| parse_usize_flag(&value, "--limit"))
                    .transpose()
                    .map_err(std::io::Error::other)?,
            };
            let report = export_audit_events(&input_path, &query, Some(&output_path), format)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            return Err(std::io::Error::other(usage()).into());
        }
    }

    Ok(())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn optional_arg_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|arg| arg == flag)?;
    args.get(pos + 1).cloned()
}

fn arg_value(args: &[String], flag: &str) -> Result<String, std::io::Error> {
    let pos = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| std::io::Error::other(format!("missing {flag}\n{}", usage())))?;
    args.get(pos + 1)
        .cloned()
        .ok_or_else(|| std::io::Error::other(format!("missing value for {flag}\n{}", usage())))
}

fn parse_u64_flag(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid integer for {flag}: `{value}`"))
}

fn parse_usize_flag(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid integer for {flag}: `{value}`"))
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(format!(
            "invalid boolean `{other}`; expected true|false|1|0"
        )),
    }
}

fn parse_operation(value: &str) -> Result<sqlrite::AccessOperation, String> {
    match value {
        "query" => Ok(sqlrite::AccessOperation::Query),
        "ingest" => Ok(sqlrite::AccessOperation::Ingest),
        "sql_admin" => Ok(sqlrite::AccessOperation::SqlAdmin),
        "delete_tenant" => Ok(sqlrite::AccessOperation::DeleteTenant),
        other => Err(format!(
            "invalid --operation `{other}`; expected query|ingest|sql_admin|delete_tenant"
        )),
    }
}

fn usage() -> String {
    "usage:\n  cargo run --bin sqlrite-security -- init-policy --path <rbac-policy.json>\n  cargo run --bin sqlrite-security -- add-key --registry <keys.json> --tenant <tenant> --key-id <id> --key-material <secret> [--active]\n  cargo run --bin sqlrite-security -- rotate-key --db <db_path> --registry <keys.json> --tenant <tenant> --field <metadata_field> --new-key-id <id> [--json]\n  cargo run --bin sqlrite-security -- verify-key --db <db_path> --registry <keys.json> --tenant <tenant> --field <metadata_field> --key-id <id>\n  cargo run --bin sqlrite-security -- export-audit --input <audit.jsonl> --output <export.jsonl|export.json> [--format jsonl|json] [--actor <id>] [--tenant <tenant>] [--operation query|ingest|sql_admin|delete_tenant] [--allowed true|false] [--from-ms <unix_ms>] [--to-ms <unix_ms>] [--limit <n>]"
        .to_string()
}
