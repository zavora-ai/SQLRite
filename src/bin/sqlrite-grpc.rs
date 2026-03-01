use sqlrite::{DurabilityProfile, GrpcServerConfig, VectorIndexMode, run_grpc_server};
use std::path::PathBuf;

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    bind_addr: String,
    profile: DurabilityProfile,
    index_mode: VectorIndexMode,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            bind_addr: "127.0.0.1:50051".to_string(),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_args(&args).map_err(std::io::Error::other)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(run_grpc_server(GrpcServerConfig {
        db_path: parsed.db_path,
        bind_addr: parsed.bind_addr,
        profile: parsed.profile,
        index_mode: parsed.index_mode,
    }))?;

    Ok(())
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
            "--bind" => {
                i += 1;
                out.bind_addr = parse_string(args, i, "--bind")?;
            }
            "--profile" => {
                i += 1;
                out.profile = parse_profile(&parse_string(args, i, "--profile")?)?;
            }
            "--index-mode" => {
                i += 1;
                out.index_mode = parse_index_mode(&parse_string(args, i, "--index-mode")?)?;
            }
            "--help" | "-h" => {
                return Err("usage: sqlrite-grpc [--db PATH] [--bind HOST:PORT] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled]".to_string())
            }
            other => {
                return Err(format!(
                    "unknown argument `{other}`\nusage: sqlrite-grpc [--db PATH] [--bind HOST:PORT] [--profile balanced|durable|fast_unsafe] [--index-mode brute_force|lsh_ann|hnsw_baseline|disabled]"
                ))
            }
        }
        i += 1;
    }

    Ok(out)
}

fn parse_string(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_profile(raw: &str) -> Result<DurabilityProfile, String> {
    match raw {
        "balanced" => Ok(DurabilityProfile::Balanced),
        "durable" => Ok(DurabilityProfile::Durable),
        "fast_unsafe" | "fast-unsafe" => Ok(DurabilityProfile::FastUnsafe),
        _ => Err(format!(
            "invalid profile `{raw}` (expected balanced|durable|fast_unsafe)"
        )),
    }
}

fn parse_index_mode(raw: &str) -> Result<VectorIndexMode, String> {
    match raw {
        "brute_force" | "bruteforce" => Ok(VectorIndexMode::BruteForce),
        "lsh_ann" | "lsh" => Ok(VectorIndexMode::LshAnn),
        "hnsw_baseline" | "hnsw" => Ok(VectorIndexMode::HnswBaseline),
        "disabled" => Ok(VectorIndexMode::Disabled),
        _ => Err(format!(
            "invalid index mode `{raw}` (expected brute_force|lsh_ann|hnsw_baseline|disabled)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_defaults_are_stable() {
        let parsed = parse_args(&[]).expect("args");
        assert_eq!(parsed.db_path, PathBuf::from("sqlrite.db"));
        assert_eq!(parsed.bind_addr, "127.0.0.1:50051");
        assert_eq!(parsed.profile, DurabilityProfile::Balanced);
    }

    #[test]
    fn parse_args_accepts_overrides() {
        let parsed = parse_args(&[
            "--db".to_string(),
            "grpc.db".to_string(),
            "--bind".to_string(),
            "0.0.0.0:50091".to_string(),
            "--profile".to_string(),
            "durable".to_string(),
            "--index-mode".to_string(),
            "hnsw_baseline".to_string(),
        ])
        .expect("args");

        assert_eq!(parsed.db_path, PathBuf::from("grpc.db"));
        assert_eq!(parsed.bind_addr, "0.0.0.0:50091");
        assert_eq!(parsed.profile, DurabilityProfile::Durable);
        assert_eq!(parsed.index_mode, VectorIndexMode::HnswBaseline);
    }
}
