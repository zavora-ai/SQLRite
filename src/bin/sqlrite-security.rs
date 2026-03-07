use sqlrite::{
    InMemoryTenantKeyRegistry, RbacPolicy, RuntimeConfig, SqlRite, TenantKey,
    rotate_tenant_encryption_key,
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

            let db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;
            let registry = InMemoryTenantKeyRegistry::load_from_json_file(&registry_path)?;
            let updated = rotate_tenant_encryption_key(
                &db,
                &tenant,
                &metadata_field,
                &registry,
                &new_key_id,
            )?;
            println!("rotated encrypted metadata for {updated} chunk(s)");
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

fn arg_value(args: &[String], flag: &str) -> Result<String, std::io::Error> {
    let pos = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| std::io::Error::other(format!("missing {flag}\n{}", usage())))?;
    args.get(pos + 1)
        .cloned()
        .ok_or_else(|| std::io::Error::other(format!("missing value for {flag}\n{}", usage())))
}

fn usage() -> String {
    "usage:\n  cargo run --bin sqlrite-security -- init-policy --path <rbac-policy.json>\n  cargo run --bin sqlrite-security -- add-key --registry <keys.json> --tenant <tenant> --key-id <id> --key-material <secret> [--active]\n  cargo run --bin sqlrite-security -- rotate-key --db <db_path> --registry <keys.json> --tenant <tenant> --field <metadata_field> --new-key-id <id>"
        .to_string()
}
