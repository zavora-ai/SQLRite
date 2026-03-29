use sqlrite::{
    DurabilityProfile, McpServerConfig, RuntimeConfig, VectorIndexMode,
    mcp_tools_manifest_document, run_stdio_mcp_server,
};
use std::path::PathBuf;

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
    auth_token: Option<String>,
    print_manifest: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
            auth_token: None,
            print_manifest: false,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_args(&args).map_err(std::io::Error::other)?;

    if parsed.print_manifest {
        let manifest = mcp_tools_manifest_document(parsed.auth_token.is_some());
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        return Ok(());
    }

    eprintln!(
        "starting SQLRite MCP stdio server (db={} auth_required={})",
        parsed.db_path.display(),
        parsed.auth_token.is_some()
    );
    run_stdio_mcp_server(McpServerConfig {
        db_path: parsed.db_path,
        runtime: RuntimeConfig {
            durability_profile: parsed.profile,
            vector_index_mode: parsed.index_mode,
            ..RuntimeConfig::default()
        },
        auth_token: parsed.auth_token,
    })
    .map_err(|error| error.into())
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(args, i, "--db")?);
            }
            "--profile" => {
                i += 1;
                out.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                out.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--auth-token" => {
                i += 1;
                out.auth_token = Some(parse_string(args, i, "--auth-token")?);
            }
            "--print-manifest" => {
                out.print_manifest = true;
            }
            "--help" | "-h" => {
                return Err(usage());
            }
            other => {
                return Err(format!("unknown argument `{other}`\n{}", usage()));
            }
        }
        i += 1;
    }

    Ok(out)
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_profile(value: &str) -> Result<DurabilityProfile, String> {
    match value {
        "balanced" => Ok(DurabilityProfile::Balanced),
        "durable" => Ok(DurabilityProfile::Durable),
        "fast_unsafe" => Ok(DurabilityProfile::FastUnsafe),
        other => Err(format!(
            "invalid --profile `{other}` (expected balanced|durable|fast_unsafe)"
        )),
    }
}

fn parse_index_mode(value: &str) -> Result<VectorIndexMode, String> {
    match value {
        "brute_force" => Ok(VectorIndexMode::BruteForce),
        "lsh_ann" => Ok(VectorIndexMode::LshAnn),
        "hnsw_baseline" | "hnsw" => Ok(VectorIndexMode::HnswBaseline),
        "disabled" => Ok(VectorIndexMode::Disabled),
        other => Err(format!(
            "invalid --index-mode `{other}` (expected brute_force|lsh_ann|hnsw_baseline|disabled)"
        )),
    }
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-mcp -- [--db PATH] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled] [--auth-token TOKEN] [--print-manifest]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_args(&[]).map_err(std::io::Error::other)?;
        assert_eq!(parsed.db_path, PathBuf::from("sqlrite.db"));
        assert_eq!(parsed.profile, DurabilityProfile::Balanced);
        assert_eq!(parsed.index_mode, VectorIndexMode::BruteForce);
        assert!(parsed.auth_token.is_none());
        Ok(())
    }

    #[test]
    fn parse_args_accepts_auth_and_manifest() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse_args(&[
            "--db".to_string(),
            "a.db".to_string(),
            "--auth-token".to_string(),
            "x".to_string(),
            "--print-manifest".to_string(),
        ])
        .map_err(std::io::Error::other)?;
        assert_eq!(parsed.db_path, PathBuf::from("a.db"));
        assert_eq!(parsed.auth_token.as_deref(), Some("x"));
        assert!(parsed.print_manifest);
        Ok(())
    }
}
