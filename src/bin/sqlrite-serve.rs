use sqlrite::{RuntimeConfig, ServerConfig, serve_health_endpoints};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;

    println!("starting sqlrite health server on {}", args.bind_addr);
    serve_health_endpoints(
        args.db_path,
        RuntimeConfig::default(),
        ServerConfig {
            bind_addr: args.bind_addr,
        },
    )
    .map_err(|e| e.into())
}

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    bind_addr: String,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        db_path: PathBuf::from("sqlrite_demo.db"),
        bind_addr: "127.0.0.1:8099".to_string(),
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" => {
                i += 1;
                out.db_path = PathBuf::from(parse_string(&args, i, "--db")?);
            }
            "--bind" => {
                i += 1;
                out.bind_addr = parse_string(&args, i, "--bind")?;
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument `{other}`\n{}", usage())),
        }
        i += 1;
    }

    Ok(out)
}

fn parse_string(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing value for {flag}\n{}", usage()))
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-serve -- [--db PATH] [--bind HOST:PORT]".to_string()
}
