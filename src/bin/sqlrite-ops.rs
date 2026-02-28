use sqlrite::{RuntimeConfig, SqlRite, backup_file, build_health_report, verify_backup_file};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args
        .first()
        .cloned()
        .ok_or_else(|| std::io::Error::other(usage()))?;

    match command.as_str() {
        "backup" => {
            let source = arg_value(&args, "--source")?;
            let destination = arg_value(&args, "--dest")?;
            backup_file(source, destination)?;
            println!("backup complete");
        }
        "verify" => {
            let path = arg_value(&args, "--path")?;
            let report = verify_backup_file(path)?;
            println!("backup verification:");
            println!("- integrity_ok={}", report.integrity_check_ok);
            println!("- chunk_count={}", report.chunk_count);
            println!("- schema_version={}", report.schema_version);
            println!("- index_mode={}", report.vector_index_mode);
        }
        "health" => {
            let db_path = arg_value(&args, "--db")?;
            let db = SqlRite::open_with_config(PathBuf::from(db_path), RuntimeConfig::default())?;
            let report = build_health_report(&db)?;
            println!("health:");
            println!("- integrity_ok={}", report.integrity_check_ok);
            println!("- chunk_count={}", report.chunk_count);
            println!("- schema_version={}", report.schema_version);
            println!("- index_mode={}", report.vector_index_mode);
            println!("- index_entries={}", report.vector_index_entries);
        }
        _ => {
            return Err(std::io::Error::other(usage()).into());
        }
    }

    Ok(())
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
    "usage:\n  cargo run --bin sqlrite-ops -- backup --source <db_path> --dest <backup_path>\n  cargo run --bin sqlrite-ops -- verify --path <backup_path>\n  cargo run --bin sqlrite-ops -- health --db <db_path>"
        .to_string()
}
