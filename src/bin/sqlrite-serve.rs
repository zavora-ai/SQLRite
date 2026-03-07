use sqlrite::{
    FailoverMode, HaRuntimeProfile, RbacPolicy, RecoveryConfig, ReplicationConfig, RuntimeConfig,
    ServerConfig, ServerRole, ServerSecurityConfig, serve_health_endpoints,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args =
        parse_args(std::env::args().skip(1).collect::<Vec<_>>()).map_err(std::io::Error::other)?;

    let replication_enabled = args.ha_role != ServerRole::Standalone || !args.peers.is_empty();
    let ha_profile = HaRuntimeProfile {
        replication: ReplicationConfig {
            enabled: replication_enabled,
            cluster_id: args.cluster_id,
            node_id: args.node_id,
            role: if replication_enabled {
                args.ha_role
            } else {
                ServerRole::Standalone
            },
            advertise_addr: args
                .advertise_addr
                .unwrap_or_else(|| args.bind_addr.clone()),
            peers: args.peers,
            sync_ack_quorum: args.sync_ack_quorum,
            heartbeat_interval_ms: args.heartbeat_interval_ms,
            election_timeout_ms: args.election_timeout_ms,
            max_replication_lag_ms: args.max_replication_lag_ms,
            failover_mode: args.failover_mode,
        },
        recovery: RecoveryConfig {
            backup_dir: args.backup_dir,
            snapshot_interval_seconds: args.snapshot_interval_seconds,
            pitr_retention_seconds: args.pitr_retention_seconds,
        },
    };
    ha_profile.validate().map_err(std::io::Error::other)?;

    println!("starting sqlrite health server on {}", args.bind_addr);
    serve_health_endpoints(
        args.db_path,
        RuntimeConfig::default(),
        ServerConfig {
            bind_addr: args.bind_addr,
            ha_profile,
            control_api_token: args.control_token,
            enable_sql_endpoint: args.enable_sql_endpoint,
            security: ServerSecurityConfig {
                secure_defaults: args.secure_defaults,
                require_auth_context: args.require_auth_context || args.secure_defaults,
                policy: if let Some(path) = &args.authz_policy_path {
                    Some(RbacPolicy::load_from_json_file(path)?)
                } else if args.secure_defaults {
                    Some(RbacPolicy::default())
                } else {
                    None
                },
                audit_log_path: args.audit_log_path.clone().or_else(|| {
                    args.secure_defaults
                        .then(|| PathBuf::from(".sqlrite/audit/server_audit.jsonl"))
                }),
                ..ServerSecurityConfig::default()
            },
        },
    )
    .map_err(|e| e.into())
}

#[derive(Debug)]
struct Args {
    db_path: PathBuf,
    bind_addr: String,
    ha_role: ServerRole,
    cluster_id: String,
    node_id: String,
    advertise_addr: Option<String>,
    peers: Vec<String>,
    sync_ack_quorum: usize,
    heartbeat_interval_ms: u64,
    election_timeout_ms: u64,
    max_replication_lag_ms: u64,
    failover_mode: FailoverMode,
    backup_dir: String,
    snapshot_interval_seconds: u64,
    pitr_retention_seconds: u64,
    control_token: Option<String>,
    enable_sql_endpoint: bool,
    secure_defaults: bool,
    require_auth_context: bool,
    authz_policy_path: Option<PathBuf>,
    audit_log_path: Option<PathBuf>,
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut out = Args {
        db_path: PathBuf::from("sqlrite_demo.db"),
        bind_addr: "127.0.0.1:8099".to_string(),
        ha_role: ServerRole::Standalone,
        cluster_id: "local-cluster".to_string(),
        node_id: "node-1".to_string(),
        advertise_addr: None,
        peers: Vec::new(),
        sync_ack_quorum: 1,
        heartbeat_interval_ms: 1_000,
        election_timeout_ms: 3_000,
        max_replication_lag_ms: 2_000,
        failover_mode: FailoverMode::Manual,
        backup_dir: "./backups".to_string(),
        snapshot_interval_seconds: 300,
        pitr_retention_seconds: 86_400,
        control_token: None,
        enable_sql_endpoint: true,
        secure_defaults: false,
        require_auth_context: false,
        authz_policy_path: None,
        audit_log_path: None,
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
            "--ha-role" => {
                i += 1;
                out.ha_role = parse_server_role(&parse_string(&args, i, "--ha-role")?)?;
            }
            "--cluster-id" => {
                i += 1;
                out.cluster_id = parse_string(&args, i, "--cluster-id")?;
            }
            "--node-id" => {
                i += 1;
                out.node_id = parse_string(&args, i, "--node-id")?;
            }
            "--advertise" => {
                i += 1;
                out.advertise_addr = Some(parse_string(&args, i, "--advertise")?);
            }
            "--peer" => {
                i += 1;
                out.peers.push(parse_string(&args, i, "--peer")?);
            }
            "--sync-ack-quorum" => {
                i += 1;
                out.sync_ack_quorum = parse_usize(&args, i, "--sync-ack-quorum")?;
            }
            "--heartbeat-ms" => {
                i += 1;
                out.heartbeat_interval_ms = parse_usize(&args, i, "--heartbeat-ms")? as u64;
            }
            "--election-timeout-ms" => {
                i += 1;
                out.election_timeout_ms = parse_usize(&args, i, "--election-timeout-ms")? as u64;
            }
            "--max-replication-lag-ms" => {
                i += 1;
                out.max_replication_lag_ms =
                    parse_usize(&args, i, "--max-replication-lag-ms")? as u64;
            }
            "--failover" => {
                i += 1;
                out.failover_mode = parse_failover_mode(&parse_string(&args, i, "--failover")?)?;
            }
            "--backup-dir" => {
                i += 1;
                out.backup_dir = parse_string(&args, i, "--backup-dir")?;
            }
            "--snapshot-interval-s" => {
                i += 1;
                out.snapshot_interval_seconds =
                    parse_usize(&args, i, "--snapshot-interval-s")? as u64;
            }
            "--pitr-retention-s" => {
                i += 1;
                out.pitr_retention_seconds = parse_usize(&args, i, "--pitr-retention-s")? as u64;
            }
            "--control-token" => {
                i += 1;
                out.control_token = Some(parse_string(&args, i, "--control-token")?);
            }
            "--disable-sql-endpoint" => {
                out.enable_sql_endpoint = false;
            }
            "--secure-defaults" => {
                out.secure_defaults = true;
            }
            "--require-auth-context" => {
                out.require_auth_context = true;
            }
            "--authz-policy" => {
                i += 1;
                out.authz_policy_path =
                    Some(PathBuf::from(parse_string(&args, i, "--authz-policy")?));
            }
            "--audit-log" => {
                i += 1;
                out.audit_log_path = Some(PathBuf::from(parse_string(&args, i, "--audit-log")?));
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

fn parse_usize(args: &[String], index: usize, flag: &str) -> Result<usize, String> {
    let raw = parse_string(args, index, flag)?;
    raw.parse::<usize>()
        .map_err(|_| format!("invalid integer for {flag}: `{raw}`\n{}", usage()))
}

fn parse_server_role(value: &str) -> Result<ServerRole, String> {
    match value {
        "standalone" => Ok(ServerRole::Standalone),
        "primary" => Ok(ServerRole::Primary),
        "replica" => Ok(ServerRole::Replica),
        other => Err(format!(
            "invalid --ha-role `{other}`; expected standalone, primary, or replica\n{}",
            usage()
        )),
    }
}

fn parse_failover_mode(value: &str) -> Result<FailoverMode, String> {
    match value {
        "manual" => Ok(FailoverMode::Manual),
        "automatic" => Ok(FailoverMode::Automatic),
        other => Err(format!(
            "invalid --failover `{other}`; expected manual or automatic\n{}",
            usage()
        )),
    }
}

fn usage() -> String {
    "usage: cargo run --bin sqlrite-serve -- [--db PATH] [--bind HOST:PORT] [--ha-role standalone|primary|replica] [--cluster-id ID] [--node-id ID] [--advertise HOST:PORT] [--peer HOST:PORT]... [--sync-ack-quorum N] [--heartbeat-ms N] [--election-timeout-ms N] [--max-replication-lag-ms N] [--failover manual|automatic] [--backup-dir DIR] [--snapshot-interval-s N] [--pitr-retention-s N] [--control-token TOKEN] [--disable-sql-endpoint] [--secure-defaults] [--require-auth-context] [--authz-policy PATH] [--audit-log PATH]".to_string()
}
