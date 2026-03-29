use crate::security::{AccessPolicy, AuditLogger};
use crate::{
    AccessContext, AccessOperation, AuditEvent, AuditExportFormat, AuditQuery, DurabilityProfile,
    FailoverMode, HaRuntimeProfile, HaRuntimeState, JsonlAuditLogger, QueryProfile, RbacPolicy,
    ReplicationLog, ReplicationLogEntry, Result, RuntimeConfig, ServerRole, SqlRite, SqlRiteError,
    build_health_report, create_backup_snapshot, execute_sdk_query, execute_sdk_sql,
    export_audit_events, list_backup_snapshots, prune_backup_snapshots,
    restore_backup_file_verified, select_backup_snapshot_for_time,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlrite_sdk_core::{QueryRequest as QueryApiRequest, SqlRequest as SqlApiRequest};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_addr: String,
    pub ha_profile: HaRuntimeProfile,
    pub control_api_token: Option<String>,
    pub enable_sql_endpoint: bool,
    pub security: ServerSecurityConfig,
}

#[derive(Debug, Clone)]
pub struct ServerSecurityConfig {
    pub secure_defaults: bool,
    pub require_auth_context: bool,
    pub policy: Option<RbacPolicy>,
    pub audit_log_path: Option<PathBuf>,
    pub audit_redacted_fields: Vec<String>,
}

impl Default for ServerSecurityConfig {
    fn default() -> Self {
        Self {
            secure_defaults: false,
            require_auth_context: false,
            policy: None,
            audit_log_path: None,
            audit_redacted_fields: vec![
                "statement".to_string(),
                "query_embedding".to_string(),
                "metadata_filters".to_string(),
                "auth_token".to_string(),
            ],
        }
    }
}

impl ServerSecurityConfig {
    fn enabled(&self) -> bool {
        self.secure_defaults
            || self.require_auth_context
            || self.policy.is_some()
            || self.audit_log_path.is_some()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8099".to_string(),
            ha_profile: HaRuntimeProfile::default(),
            control_api_token: None,
            enable_sql_endpoint: true,
            security: ServerSecurityConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct ControlPlaneState {
    profile: HaRuntimeProfile,
    runtime: HaRuntimeState,
    replication_log: ReplicationLog,
    replica_progress: HashMap<String, u64>,
    resilience: ResilienceState,
    chaos: ChaosHarnessState,
    observability: ObservabilityState,
}

impl ControlPlaneState {
    fn new(profile: HaRuntimeProfile) -> Self {
        let runtime = HaRuntimeState::new(&profile);
        Self {
            profile,
            runtime,
            replication_log: ReplicationLog::new(),
            replica_progress: HashMap::new(),
            resilience: ResilienceState::default(),
            chaos: ChaosHarnessState::default(),
            observability: ObservabilityState::default(),
        }
    }

    fn snapshot_json(&self) -> Value {
        json!({
            "profile": self.profile,
            "state": self.runtime,
            "replication": {
                "log_len": self.replication_log.len(),
                "last_log_index": self.replication_log.last_index(),
                "last_log_term": self.replication_log.last_term(),
                "replica_progress": self.replica_progress,
            },
            "resilience": self.resilience.to_json(self.chaos.active_count()),
            "chaos": self.chaos.to_json(),
            "observability": self.observability.to_json(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct QueryTraceRecord {
    seq: u64,
    timestamp_unix_ms: u64,
    method: String,
    path: String,
    status: u16,
    duration_ms: u64,
}

#[derive(Debug, Clone)]
struct ObservabilityState {
    request_seq: u64,
    requests_total: u64,
    requests_server_errors_total: u64,
    requests_client_errors_total: u64,
    sql_requests_total: u64,
    sql_requests_failed_total: u64,
    sql_latency_total_ms: u128,
    sql_latency_max_ms: u64,
    alert_simulations_total: u64,
    traces: VecDeque<QueryTraceRecord>,
    trace_capacity: usize,
}

impl Default for ObservabilityState {
    fn default() -> Self {
        Self {
            request_seq: 0,
            requests_total: 0,
            requests_server_errors_total: 0,
            requests_client_errors_total: 0,
            sql_requests_total: 0,
            sql_requests_failed_total: 0,
            sql_latency_total_ms: 0,
            sql_latency_max_ms: 0,
            alert_simulations_total: 0,
            traces: VecDeque::new(),
            trace_capacity: 512,
        }
    }
}

impl ObservabilityState {
    fn record_request(&mut self, method: &str, path: &str, status: u16, duration_ms: u64) {
        self.request_seq = self.request_seq.saturating_add(1);
        self.requests_total = self.requests_total.saturating_add(1);
        if status >= 500 {
            self.requests_server_errors_total = self.requests_server_errors_total.saturating_add(1);
        } else if status >= 400 {
            self.requests_client_errors_total = self.requests_client_errors_total.saturating_add(1);
        }

        let (raw_path, _) = split_path_and_query(path);
        if matches!(
            raw_path,
            "/v1/sql"
                | "/v1/query"
                | "/v1/query-compact"
                | "/grpc/sqlrite.v1.QueryService/Query"
                | "/grpc/sqlrite.v1.QueryService/Sql"
        ) {
            self.sql_requests_total = self.sql_requests_total.saturating_add(1);
            self.sql_latency_total_ms = self
                .sql_latency_total_ms
                .saturating_add(duration_ms as u128);
            self.sql_latency_max_ms = self.sql_latency_max_ms.max(duration_ms);
            if status >= 400 {
                self.sql_requests_failed_total = self.sql_requests_failed_total.saturating_add(1);
            }
        }

        self.traces.push_back(QueryTraceRecord {
            seq: self.request_seq,
            timestamp_unix_ms: unix_ms_now(),
            method: method.to_string(),
            path: raw_path.to_string(),
            status,
            duration_ms,
        });
        while self.traces.len() > self.trace_capacity {
            let _ = self.traces.pop_front();
        }
    }

    fn sql_avg_latency_ms(&self) -> f64 {
        if self.sql_requests_total == 0 {
            return 0.0;
        }
        self.sql_latency_total_ms as f64 / self.sql_requests_total as f64
    }

    fn recent_traces(&self, limit: usize) -> Vec<QueryTraceRecord> {
        let take = limit.max(1).min(self.trace_capacity);
        self.traces.iter().rev().take(take).cloned().collect()
    }

    fn to_json(&self) -> Value {
        json!({
            "requests_total": self.requests_total,
            "requests_server_errors_total": self.requests_server_errors_total,
            "requests_client_errors_total": self.requests_client_errors_total,
            "sql_requests_total": self.sql_requests_total,
            "sql_requests_failed_total": self.sql_requests_failed_total,
            "sql_avg_latency_ms": self.sql_avg_latency_ms(),
            "sql_latency_max_ms": self.sql_latency_max_ms,
            "alert_simulations_total": self.alert_simulations_total,
            "trace_buffered": self.traces.len(),
            "trace_capacity": self.trace_capacity,
        })
    }

    fn reset(&mut self) {
        self.requests_total = 0;
        self.requests_server_errors_total = 0;
        self.requests_client_errors_total = 0;
        self.sql_requests_total = 0;
        self.sql_requests_failed_total = 0;
        self.sql_latency_total_ms = 0;
        self.sql_latency_max_ms = 0;
        self.traces.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ChaosScenario {
    NodeCrash,
    DiskFull,
    PartitionSubset,
}

impl ChaosScenario {
    fn as_str(self) -> &'static str {
        match self {
            Self::NodeCrash => "node_crash",
            Self::DiskFull => "disk_full",
            Self::PartitionSubset => "partition_subset",
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
struct ChaosFaultState {
    scenario: String,
    injected_at_unix_ms: u64,
    duration_ms: Option<u64>,
    note: Option<String>,
    blocked_nodes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct ChaosHarnessState {
    active_faults: HashMap<ChaosScenario, ChaosFaultState>,
}

impl ChaosHarnessState {
    fn inject(&mut self, request: ChaosInjectRequest) {
        let scenario = request.scenario;
        self.active_faults.insert(
            scenario,
            ChaosFaultState {
                scenario: scenario.as_str().to_string(),
                injected_at_unix_ms: unix_ms_now(),
                duration_ms: request.duration_ms,
                note: request.note,
                blocked_nodes: request.blocked_nodes.unwrap_or_default(),
            },
        );
    }

    fn clear(&mut self, scenario: Option<ChaosScenario>) {
        if let Some(value) = scenario {
            self.active_faults.remove(&value);
        } else {
            self.active_faults.clear();
        }
    }

    fn cleanup_expired(&mut self, now_ms: u64) {
        self.active_faults.retain(|_, fault| {
            let Some(duration_ms) = fault.duration_ms else {
                return true;
            };
            now_ms.saturating_sub(fault.injected_at_unix_ms) < duration_ms
        });
    }

    fn has(&self, scenario: ChaosScenario) -> bool {
        self.active_faults.contains_key(&scenario)
    }

    fn active_count(&self) -> usize {
        self.active_faults.len()
    }

    fn to_json(&self) -> Value {
        let faults = self
            .active_faults
            .values()
            .cloned()
            .map(|fault| serde_json::to_value(fault).unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        json!({
            "active_fault_count": faults.len(),
            "faults": faults,
        })
    }
}

#[derive(Debug, Clone, Default)]
struct ResilienceState {
    failover_events_total: u64,
    failover_completed_total: u64,
    active_failover_started_unix_ms: Option<u64>,
    last_failover_duration_ms: Option<u64>,
    cumulative_failover_duration_ms: u128,
    restore_events_total: u64,
    restore_completed_total: u64,
    active_restore_started_unix_ms: Option<u64>,
    last_restore_duration_ms: Option<u64>,
    cumulative_restore_duration_ms: u128,
    chaos_injections_total: u64,
    chaos_blocked_requests_total: u64,
}

impl ResilienceState {
    fn start_failover(&mut self) {
        if self.active_failover_started_unix_ms.is_none() {
            self.failover_events_total = self.failover_events_total.saturating_add(1);
            self.active_failover_started_unix_ms = Some(unix_ms_now());
        }
    }

    fn complete_failover(&mut self) -> Option<u64> {
        let start = self.active_failover_started_unix_ms.take()?;
        let duration = unix_ms_now().saturating_sub(start);
        self.failover_completed_total = self.failover_completed_total.saturating_add(1);
        self.last_failover_duration_ms = Some(duration);
        self.cumulative_failover_duration_ms = self
            .cumulative_failover_duration_ms
            .saturating_add(duration as u128);
        Some(duration)
    }

    fn start_restore(&mut self) {
        if self.active_restore_started_unix_ms.is_none() {
            self.restore_events_total = self.restore_events_total.saturating_add(1);
            self.active_restore_started_unix_ms = Some(unix_ms_now());
        }
    }

    fn complete_restore(&mut self) -> Option<u64> {
        let start = self.active_restore_started_unix_ms.take()?;
        let duration = unix_ms_now().saturating_sub(start);
        self.restore_completed_total = self.restore_completed_total.saturating_add(1);
        self.last_restore_duration_ms = Some(duration);
        self.cumulative_restore_duration_ms = self
            .cumulative_restore_duration_ms
            .saturating_add(duration as u128);
        Some(duration)
    }

    fn avg_failover_duration_ms(&self) -> f64 {
        if self.failover_completed_total == 0 {
            return 0.0;
        }
        self.cumulative_failover_duration_ms as f64 / self.failover_completed_total as f64
    }

    fn avg_restore_duration_ms(&self) -> f64 {
        if self.restore_completed_total == 0 {
            return 0.0;
        }
        self.cumulative_restore_duration_ms as f64 / self.restore_completed_total as f64
    }

    fn to_json(&self, active_chaos_faults: usize) -> Value {
        json!({
            "failover_events_total": self.failover_events_total,
            "failover_completed_total": self.failover_completed_total,
            "active_failover_started_unix_ms": self.active_failover_started_unix_ms,
            "last_failover_duration_ms": self.last_failover_duration_ms,
            "avg_failover_duration_ms": self.avg_failover_duration_ms(),
            "restore_events_total": self.restore_events_total,
            "restore_completed_total": self.restore_completed_total,
            "active_restore_started_unix_ms": self.active_restore_started_unix_ms,
            "last_restore_duration_ms": self.last_restore_duration_ms,
            "avg_restore_duration_ms": self.avg_restore_duration_ms(),
            "chaos_injections_total": self.chaos_injections_total,
            "chaos_blocked_requests_total": self.chaos_blocked_requests_total,
            "chaos_active_faults": active_chaos_faults,
        })
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug, Deserialize, Default)]
struct PromoteRequest {
    leader_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct StepDownRequest {
    leader_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RecoveryRequest {
    backup_artifact: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RecoveryStartRequest {
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RecoverySnapshotRequest {
    note: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RecoveryVerifyRestoreRequest {
    snapshot_path: Option<String>,
    target_unix_ms: Option<u64>,
    note: Option<String>,
    #[serde(default)]
    keep_artifact: bool,
}

#[derive(Debug, Deserialize, Default)]
struct RecoveryPruneRequest {
    retention_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ReplicationAppendRequest {
    operation: Option<String>,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct ReplicationReceiveRequest {
    term: u64,
    leader_id: String,
    prev_log_index: u64,
    prev_log_term: u64,
    #[serde(default)]
    entries: Vec<ReplicationLogEntry>,
    leader_commit: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ReplicationAckRequest {
    node_id: String,
    index: u64,
}

#[derive(Debug, Deserialize, Default)]
struct ReplicationReconcileRequest {
    node_id: Option<String>,
    last_applied_index: Option<u64>,
    commit_index: Option<u64>,
    replication_lag_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ElectionVoteRequest {
    term: u64,
    candidate_id: String,
    candidate_last_log_index: u64,
    candidate_last_log_term: u64,
}

#[derive(Debug, Deserialize)]
struct ElectionHeartbeatRequest {
    term: u64,
    leader_id: String,
    commit_index: u64,
    leader_last_log_index: Option<u64>,
    replication_lag_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct AutoFailoverCheckRequest {
    #[serde(default)]
    force: bool,
    simulate_elapsed_ms: Option<u64>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChaosInjectRequest {
    scenario: ChaosScenario,
    duration_ms: Option<u64>,
    note: Option<String>,
    blocked_nodes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct ChaosClearRequest {
    scenario: Option<ChaosScenario>,
}

#[derive(Debug, Deserialize, Default)]
struct AlertSimulationRequest {
    sql_error_rate: Option<f64>,
    sql_avg_latency_ms: Option<f64>,
    replication_lag_ms: Option<u64>,
    restore_active_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct SecurityAuditExportRequest {
    actor_id: Option<String>,
    tenant_id: Option<String>,
    operation: Option<AccessOperation>,
    allowed: Option<bool>,
    from_unix_ms: Option<u64>,
    to_unix_ms: Option<u64>,
    limit: Option<usize>,
    output_path: Option<String>,
    format: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RerankHookRequest {
    query_text: Option<String>,
    query_embedding: Option<Vec<f32>>,
    candidate_count: Option<usize>,
    alpha: Option<f32>,
    candidate_limit: Option<usize>,
    query_profile: Option<String>,
    metadata_filters: Option<HashMap<String, String>>,
    doc_id: Option<String>,
}

pub fn serve_health_endpoints(
    db_path: impl AsRef<Path>,
    runtime: RuntimeConfig,
    config: ServerConfig,
) -> Result<()> {
    config
        .ha_profile
        .validate()
        .map_err(std::io::Error::other)?;

    let db_path = db_path.as_ref().to_path_buf();
    let db = SqlRite::open_with_config(&db_path, runtime.clone())?;
    let listener = TcpListener::bind(&config.bind_addr)?;

    let mut control = ControlPlaneState::new(config.ha_profile.clone());
    restore_replication_state(&db_path, &mut control)?;
    let state = Arc::new(Mutex::new(control));

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(_) => continue,
        };

        if let Err(error) = handle_connection(
            &db,
            &db_path,
            runtime.durability_profile,
            &config,
            &state,
            &mut stream,
        ) {
            let _ = write_response(
                &mut stream,
                500,
                "text/plain; charset=utf-8",
                &format!("internal error: {error}"),
            );
        }
    }

    Ok(())
}

fn handle_connection(
    db: &SqlRite,
    db_path: &Path,
    sql_profile: DurabilityProfile,
    config: &ServerConfig,
    state: &Arc<Mutex<ControlPlaneState>>,
    stream: &mut TcpStream,
) -> Result<()> {
    let started_unix_ms = unix_ms_now();
    let request = read_http_request(stream)?;
    let (status, content_type, body) =
        build_response(db, db_path, sql_profile, config, state, &request)?;
    write_response(stream, status, content_type, &body)?;
    if let Ok(mut control) = state.lock() {
        let duration_ms = unix_ms_now().saturating_sub(started_unix_ms);
        control
            .observability
            .record_request(&request.method, &request.path, status, duration_ms);
    }
    Ok(())
}

fn read_http_request(stream: &TcpStream) -> Result<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first_line = String::new();
    reader.read_line(&mut first_line)?;

    let (method, path) = parse_http_request_line(&first_line)
        .ok_or_else(|| std::io::Error::other("invalid HTTP request line"))?;

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(
                name.trim().to_ascii_lowercase(),
                value.trim().trim_end_matches('\r').to_string(),
            );
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(0);

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers,
        body,
    })
}

fn parse_http_request_line(first_line: &str) -> Option<(&str, &str)> {
    let mut parts = first_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    Some((method, path))
}

fn build_response(
    db: &SqlRite,
    db_path: &Path,
    sql_profile: DurabilityProfile,
    config: &ServerConfig,
    state: &Arc<Mutex<ControlPlaneState>>,
    request: &HttpRequest,
) -> Result<(u16, &'static str, String)> {
    let (path, query) = split_path_and_query(&request.path);
    let now_ms = unix_ms_now();

    let chaos_control_endpoint = matches!(
        path,
        "/control/v1/chaos/status" | "/control/v1/chaos/inject" | "/control/v1/chaos/clear"
    );
    {
        let mut control = state
            .lock()
            .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
        control.chaos.cleanup_expired(now_ms);
        if !chaos_control_endpoint
            && let Some(blocked) = chaos_blocking_response(&mut control, request, path)
        {
            return Ok(blocked);
        }

        let skip_auto_failover = matches!(
            path,
            "/control/v1/election/heartbeat"
                | "/control/v1/replication/receive"
                | "/control/v1/failover/auto-check"
        );
        if !skip_auto_failover {
            let _ = maybe_trigger_automatic_failover(
                db_path,
                &mut control,
                AutoFailoverEvalInput {
                    force: false,
                    simulated_elapsed_ms: None,
                    reason: Some("periodic_request_tick".to_string()),
                },
            )?;
        }
    }

    match (request.method.as_str(), path) {
        ("GET", "/healthz") => {
            let report = build_health_report(db)?;
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "storage": report,
                "ha": control.snapshot_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/readyz") => {
            let report = build_health_report(db)?;
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let mut ready = report.integrity_check_ok;
            if control.profile.replication.enabled
                && control.runtime.role != ServerRole::Primary
                && control.runtime.leader_id.is_none()
            {
                ready = false;
            }

            let status = if ready { 200 } else { 503 };
            let payload = json!({
                "ready": ready,
                "schema_version": report.schema_version,
                "ha_enabled": control.profile.replication.enabled,
                "role": control.runtime.role,
                "leader_id": control.runtime.leader_id,
                "term": control.runtime.current_term,
                "commit_index": control.runtime.commit_index,
                "last_log_index": control.runtime.last_log_index,
                "active_chaos_faults": control.chaos.active_count(),
            });
            Ok((status, "application/json", payload.to_string()))
        }
        ("GET", "/metrics") => {
            let report = build_health_report(db)?;
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let role_metric = match control.runtime.role {
                ServerRole::Standalone => 0,
                ServerRole::Primary => 1,
                ServerRole::Replica => 2,
            };

            let body = format!(
                "sqlrite_chunk_count {}\n\
                 sqlrite_schema_version {}\n\
                 sqlrite_index_entries {}\n\
                 sqlrite_ha_enabled {}\n\
                 sqlrite_ha_role {}\n\
                 sqlrite_ha_term {}\n\
                 sqlrite_ha_commit_index {}\n\
                 sqlrite_ha_last_log_index {}\n\
                 sqlrite_ha_last_log_term {}\n\
                 sqlrite_ha_replication_log_entries {}\n\
                 sqlrite_ha_replication_lag_ms {}\n\
                 sqlrite_ha_failover_in_progress {}\n\
                 sqlrite_ha_failover_events_total {}\n\
                 sqlrite_ha_failover_completed_total {}\n\
                 sqlrite_ha_failover_last_duration_ms {}\n\
                 sqlrite_ha_failover_avg_duration_ms {}\n\
                 sqlrite_ha_restore_events_total {}\n\
                 sqlrite_ha_restore_completed_total {}\n\
                 sqlrite_ha_restore_last_duration_ms {}\n\
                 sqlrite_ha_restore_avg_duration_ms {}\n\
                 sqlrite_ha_chaos_faults_active {}\n\
                 sqlrite_ha_chaos_injections_total {}\n\
                 sqlrite_ha_chaos_blocked_requests_total {}\n\
                 sqlrite_requests_total {}\n\
                 sqlrite_requests_server_errors_total {}\n\
                 sqlrite_requests_client_errors_total {}\n\
                 sqlrite_requests_sql_total {}\n\
                 sqlrite_requests_sql_errors_total {}\n\
                 sqlrite_requests_sql_avg_latency_ms {}\n\
                 sqlrite_requests_sql_max_latency_ms {}\n\
                 sqlrite_observability_traces_buffered {}\n\
                 sqlrite_alert_simulations_total {}\n",
                report.chunk_count,
                report.schema_version,
                report.vector_index_entries,
                if control.profile.replication.enabled {
                    1
                } else {
                    0
                },
                role_metric,
                control.runtime.current_term,
                control.runtime.commit_index,
                control.runtime.last_log_index,
                control.runtime.last_log_term,
                control.replication_log.len(),
                control.runtime.replication_lag_ms,
                if control.runtime.failover_in_progress {
                    1
                } else {
                    0
                },
                control.resilience.failover_events_total,
                control.resilience.failover_completed_total,
                control.resilience.last_failover_duration_ms.unwrap_or(0),
                control.resilience.avg_failover_duration_ms(),
                control.resilience.restore_events_total,
                control.resilience.restore_completed_total,
                control.resilience.last_restore_duration_ms.unwrap_or(0),
                control.resilience.avg_restore_duration_ms(),
                control.chaos.active_count(),
                control.resilience.chaos_injections_total,
                control.resilience.chaos_blocked_requests_total,
                control.observability.requests_total,
                control.observability.requests_server_errors_total,
                control.observability.requests_client_errors_total,
                control.observability.sql_requests_total,
                control.observability.sql_requests_failed_total,
                control.observability.sql_avg_latency_ms(),
                control.observability.sql_latency_max_ms,
                control.observability.traces.len(),
                control.observability.alert_simulations_total,
            );
            Ok((200, "text/plain; version=0.0.4", body))
        }
        ("GET", "/control/v1/profile") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            Ok((
                200,
                "application/json",
                serde_json::to_string(&control.profile)?,
            ))
        }
        ("GET", "/control/v1/security") => Ok((
            200,
            "application/json",
            security_summary_json(config).to_string(),
        )),
        ("POST", "/control/v1/security/audit/export") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let Some(audit_path) = &config.security.audit_log_path else {
                return Ok((
                    400,
                    "application/json",
                    json!({"error": "audit log path is not configured"}).to_string(),
                ));
            };
            let input = parse_optional_json_body::<SecurityAuditExportRequest>(request)
                .map_err(std::io::Error::other)?;
            let format = match input.format.as_deref() {
                Some("json") => AuditExportFormat::Json,
                Some("jsonl") | None => AuditExportFormat::Jsonl,
                Some(other) => return Ok((
                    400,
                    "application/json",
                    json!({"error": format!("invalid format `{other}`; expected json or jsonl")})
                        .to_string(),
                )),
            };
            let report = export_audit_events(
                audit_path,
                &AuditQuery {
                    actor_id: input.actor_id,
                    tenant_id: input.tenant_id,
                    operation: input.operation,
                    allowed: input.allowed,
                    from_unix_ms: input.from_unix_ms,
                    to_unix_ms: input.to_unix_ms,
                    limit: input.limit,
                },
                input.output_path.as_deref().map(Path::new),
                format,
            )?;
            Ok((200, "application/json", serde_json::to_string(&report)?))
        }
        ("GET", "/control/v1/state") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            Ok((200, "application/json", control.snapshot_json().to_string()))
        }
        ("GET", "/control/v1/peers") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "node_id": control.profile.replication.node_id,
                "advertise_addr": control.profile.replication.advertise_addr,
                "peers": control.profile.replication.peers,
                "sync_ack_quorum": control.profile.replication.sync_ack_quorum,
                "replica_progress": control.replica_progress,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/replication/log") => {
            let from = query
                .get("from")
                .and_then(|raw| raw.parse::<u64>().ok())
                .unwrap_or(1);
            let limit = query
                .get("limit")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(256)
                .min(1_024);

            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "from": from,
                "limit": limit,
                "entries": control.replication_log.entries_from(from, limit),
                "last_log_index": control.runtime.last_log_index,
                "last_log_term": control.runtime.last_log_term,
                "commit_index": control.runtime.commit_index,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/resilience") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            Ok((
                200,
                "application/json",
                control
                    .resilience
                    .to_json(control.chaos.active_count())
                    .to_string(),
            ))
        }
        ("GET", "/control/v1/observability/metrics-map") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "metrics": [
                    {"name":"sqlrite_chunk_count","group":"storage","type":"gauge"},
                    {"name":"sqlrite_schema_version","group":"storage","type":"gauge"},
                    {"name":"sqlrite_index_entries","group":"retrieval","type":"gauge"},
                    {"name":"sqlrite_ha_enabled","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_role","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_term","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_commit_index","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_last_log_index","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_last_log_term","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_replication_lag_ms","group":"ha","type":"gauge"},
                    {"name":"sqlrite_ha_failover_events_total","group":"resilience","type":"counter"},
                    {"name":"sqlrite_ha_failover_completed_total","group":"resilience","type":"counter"},
                    {"name":"sqlrite_ha_restore_events_total","group":"resilience","type":"counter"},
                    {"name":"sqlrite_ha_restore_completed_total","group":"resilience","type":"counter"},
                    {"name":"sqlrite_ha_chaos_injections_total","group":"chaos","type":"counter"},
                    {"name":"sqlrite_ha_chaos_blocked_requests_total","group":"chaos","type":"counter"},
                    {"name":"sqlrite_requests_total","group":"http","type":"counter"},
                    {"name":"sqlrite_requests_server_errors_total","group":"http","type":"counter"},
                    {"name":"sqlrite_requests_client_errors_total","group":"http","type":"counter"},
                    {"name":"sqlrite_requests_sql_total","group":"sql","type":"counter"},
                    {"name":"sqlrite_requests_sql_errors_total","group":"sql","type":"counter"},
                    {"name":"sqlrite_requests_sql_avg_latency_ms","group":"sql","type":"gauge"},
                    {"name":"sqlrite_requests_sql_max_latency_ms","group":"sql","type":"gauge"},
                    {"name":"sqlrite_observability_traces_buffered","group":"observability","type":"gauge"},
                    {"name":"sqlrite_alert_simulations_total","group":"observability","type":"counter"}
                ],
                "state": control.observability.to_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/traces/recent") => {
            let limit = query
                .get("limit")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(50)
                .min(512);
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "limit": limit,
                "traces": control.observability.recent_traces(limit),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/observability/reset") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            control.observability.reset();
            let payload = json!({
                "status": "reset",
                "observability": control.observability.to_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/alerts/templates") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "templates": [
                    {
                        "id": "sql_error_rate_high",
                        "severity": "warning",
                        "threshold": {"sql_error_rate_gt": 0.05},
                        "description": "SQL API error rate exceeded 5%"
                    },
                    {
                        "id": "sql_latency_high",
                        "severity": "warning",
                        "threshold": {"sql_avg_latency_ms_gt": 50.0},
                        "description": "SQL API average latency exceeded 50ms"
                    },
                    {
                        "id": "replication_lag_high",
                        "severity": "critical",
                        "threshold": {"replication_lag_ms_gt": control.profile.replication.max_replication_lag_ms},
                        "description": "Replication lag exceeded configured max_replication_lag_ms"
                    },
                    {
                        "id": "restore_stuck",
                        "severity": "critical",
                        "threshold": {"restore_active_ms_gt": control.profile.recovery.snapshot_interval_seconds.saturating_mul(1_000)},
                        "description": "Restore workflow appears stuck beyond snapshot interval"
                    }
                ]
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/alerts/simulate") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let input = parse_optional_json_body::<AlertSimulationRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;

            let observed_sql_error_rate = if control.observability.sql_requests_total == 0 {
                0.0
            } else {
                control.observability.sql_requests_failed_total as f64
                    / control.observability.sql_requests_total as f64
            };
            let observed_restore_active_ms = control
                .resilience
                .active_restore_started_unix_ms
                .map(|start| now_ms.saturating_sub(start))
                .unwrap_or(0);
            let eval_sql_error_rate = input.sql_error_rate.unwrap_or(observed_sql_error_rate);
            let eval_sql_avg_latency_ms = input
                .sql_avg_latency_ms
                .unwrap_or(control.observability.sql_avg_latency_ms());
            let eval_replication_lag_ms = input
                .replication_lag_ms
                .unwrap_or(control.runtime.replication_lag_ms);
            let eval_restore_active_ms = input
                .restore_active_ms
                .unwrap_or(observed_restore_active_ms);

            let mut fired = Vec::<Value>::new();
            if eval_sql_error_rate > 0.05 {
                fired.push(json!({"id":"sql_error_rate_high","severity":"warning","value":eval_sql_error_rate}));
            }
            if eval_sql_avg_latency_ms > 50.0 {
                fired.push(json!({"id":"sql_latency_high","severity":"warning","value":eval_sql_avg_latency_ms}));
            }
            if eval_replication_lag_ms > control.profile.replication.max_replication_lag_ms {
                fired.push(json!({"id":"replication_lag_high","severity":"critical","value":eval_replication_lag_ms}));
            }
            if eval_restore_active_ms
                > control
                    .profile
                    .recovery
                    .snapshot_interval_seconds
                    .saturating_mul(1_000)
            {
                fired.push(json!({"id":"restore_stuck","severity":"critical","value":eval_restore_active_ms}));
            }
            control.observability.alert_simulations_total = control
                .observability
                .alert_simulations_total
                .saturating_add(1);
            let payload = json!({
                "fired_alerts": fired,
                "evaluated": {
                    "sql_error_rate": eval_sql_error_rate,
                    "sql_avg_latency_ms": eval_sql_avg_latency_ms,
                    "replication_lag_ms": eval_replication_lag_ms,
                    "restore_active_ms": eval_restore_active_ms,
                },
                "simulation_count": control.observability.alert_simulations_total,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/slo/report") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let availability_percent = if control.observability.requests_total == 0 {
                100.0
            } else {
                let successful = control
                    .observability
                    .requests_total
                    .saturating_sub(control.observability.requests_server_errors_total);
                (successful as f64 / control.observability.requests_total as f64) * 100.0
            };
            let rpo_seconds = (control.runtime.replication_lag_ms as f64) / 1_000.0;
            let payload = json!({
                "availability": {
                    "observed_percent": availability_percent,
                    "target_percent": 99.95,
                    "passes_target": availability_percent >= 99.95,
                    "requests_total": control.observability.requests_total,
                    "server_errors_total": control.observability.requests_server_errors_total,
                },
                "rpo": {
                    "observed_seconds": rpo_seconds,
                    "target_seconds": 60.0,
                    "passes_target": rpo_seconds <= 60.0,
                },
                "resilience": control.resilience.to_json(control.chaos.active_count()),
                "observability": control.observability.to_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/failover/status") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let heartbeat_age_ms = control
                .runtime
                .last_heartbeat_unix_ms
                .map(|last| now_ms.saturating_sub(last));
            let payload = json!({
                "automatic_enabled": control.profile.replication.failover_mode == FailoverMode::Automatic,
                "heartbeat_age_ms": heartbeat_age_ms,
                "election_timeout_ms": control.profile.replication.election_timeout_ms,
                "role": control.runtime.role,
                "leader_id": control.runtime.leader_id,
                "failover_in_progress": control.runtime.failover_in_progress,
                "resilience": control.resilience.to_json(control.chaos.active_count()),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/failover/auto-check") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let check = parse_optional_json_body::<AutoFailoverCheckRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let event = maybe_trigger_automatic_failover(
                db_path,
                &mut control,
                AutoFailoverEvalInput {
                    force: check.force,
                    simulated_elapsed_ms: check.simulate_elapsed_ms,
                    reason: check.reason,
                },
            )?;
            let payload = json!({
                "triggered": event.is_some(),
                "event": event,
                "state": control.runtime,
                "resilience": control.resilience.to_json(control.chaos.active_count()),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/failover/start") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            control.runtime.mark_failover_started();
            control.resilience.start_failover();
            persist_resilience_event(db_path, "failover_started", None, "manual_start")?;
            persist_runtime_marker(db_path, "failover_started", "true")?;
            Ok((
                200,
                "application/json",
                serde_json::to_string(&control.runtime)?,
            ))
        }
        ("POST", "/control/v1/failover/promote") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let promote = parse_optional_json_body::<PromoteRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;

            let leader = promote
                .leader_id
                .unwrap_or_else(|| control.profile.replication.node_id.clone());
            control.profile.replication.enabled = true;
            control.profile.replication.role = ServerRole::Primary;
            control.runtime.promote_to_primary(leader);
            if let Some(duration_ms) = control.resilience.complete_failover() {
                persist_resilience_event(
                    db_path,
                    "failover_completed",
                    Some(duration_ms),
                    "manual_promote",
                )?;
            }
            persist_runtime_marker(
                db_path,
                "last_role_transition",
                &serde_json::to_string(&control.runtime)?,
            )?;
            Ok((
                200,
                "application/json",
                serde_json::to_string(&control.runtime)?,
            ))
        }
        ("POST", "/control/v1/failover/step-down") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let step_down = parse_optional_json_body::<StepDownRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;

            control.profile.replication.enabled = true;
            control.profile.replication.role = ServerRole::Replica;
            control.runtime.step_down_to_replica(step_down.leader_id);
            persist_runtime_marker(
                db_path,
                "last_role_transition",
                &serde_json::to_string(&control.runtime)?,
            )?;
            Ok((
                200,
                "application/json",
                serde_json::to_string(&control.runtime)?,
            ))
        }
        ("POST", "/control/v1/recovery/mark-restored") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let recovery = parse_optional_json_body::<RecoveryRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;

            let event = format!(
                "backup_artifact={} note={}",
                recovery
                    .backup_artifact
                    .unwrap_or_else(|| "<unspecified>".to_string()),
                recovery.note.unwrap_or_else(|| "<none>".to_string())
            );
            control.runtime.mark_recovery_event(event.clone());
            if control.resilience.active_restore_started_unix_ms.is_none() {
                control.resilience.start_restore();
            }
            if let Some(duration_ms) = control.resilience.complete_restore() {
                persist_resilience_event(
                    db_path,
                    "restore_completed",
                    Some(duration_ms),
                    "recovery_mark_restored",
                )?;
            }
            persist_runtime_marker(db_path, "last_recovery_event", &event)?;

            Ok((
                200,
                "application/json",
                serde_json::to_string(&control.runtime)?,
            ))
        }
        ("POST", "/control/v1/recovery/start") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let recovery = parse_optional_json_body::<RecoveryStartRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            control.resilience.start_restore();
            let note = recovery
                .note
                .unwrap_or_else(|| "restore_started".to_string());
            persist_resilience_event(db_path, "restore_started", None, &note)?;
            Ok((
                200,
                "application/json",
                json!({
                    "restore_in_progress": true,
                    "started_unix_ms": control.resilience.active_restore_started_unix_ms,
                })
                .to_string(),
            ))
        }
        ("POST", "/control/v1/recovery/snapshot") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let input = parse_optional_json_body::<RecoverySnapshotRequest>(request)
                .map_err(std::io::Error::other)?;
            let backup_dir = {
                let control = state
                    .lock()
                    .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
                control.profile.recovery.backup_dir.clone()
            };
            let snapshot = create_backup_snapshot(db_path, &backup_dir, input.note.as_deref())?;
            let payload = json!({
                "snapshot": snapshot,
                "backup_dir": backup_dir,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/recovery/snapshots") => {
            let backup_dir = {
                let control = state
                    .lock()
                    .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
                control.profile.recovery.backup_dir.clone()
            };
            let limit = query
                .get("limit")
                .and_then(|raw| raw.parse::<usize>().ok())
                .unwrap_or(100)
                .min(1_000);
            let mut snapshots = list_backup_snapshots(&backup_dir)?;
            if snapshots.len() > limit {
                snapshots.truncate(limit);
            }
            let payload = json!({
                "backup_dir": backup_dir,
                "count": snapshots.len(),
                "snapshots": snapshots,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/recovery/verify-restore") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let input = parse_optional_json_body::<RecoveryVerifyRestoreRequest>(request)
                .map_err(std::io::Error::other)?;
            let backup_dir = {
                let control = state
                    .lock()
                    .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
                control.profile.recovery.backup_dir.clone()
            };
            let snapshots = list_backup_snapshots(&backup_dir)?;
            let selected = if let Some(snapshot_path) = input.snapshot_path.clone() {
                snapshots
                    .into_iter()
                    .find(|snapshot| snapshot.snapshot_path == snapshot_path)
                    .ok_or_else(|| {
                        std::io::Error::other("requested snapshot_path is not in backup catalog")
                    })?
            } else if let Some(target) = input.target_unix_ms {
                select_backup_snapshot_for_time(&backup_dir, target)?.ok_or_else(|| {
                    std::io::Error::other(
                        "no snapshot exists at or before requested target_unix_ms",
                    )
                })?
            } else {
                snapshots.into_iter().next().ok_or_else(|| {
                    std::io::Error::other("no snapshots available for restore verification")
                })?
            };
            let verify_dir = Path::new(&backup_dir).join("restore_verification");
            std::fs::create_dir_all(&verify_dir)?;
            let target_path = verify_dir.join(format!(
                "verify-{}-{}.db",
                selected.snapshot_id,
                unix_ms_now()
            ));
            let report = restore_backup_file_verified(&selected.snapshot_path, &target_path)?;
            let note = input.note.unwrap_or_else(|| "verify_restore".to_string());
            {
                let mut control = state
                    .lock()
                    .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
                if control.resilience.active_restore_started_unix_ms.is_none() {
                    control.resilience.start_restore();
                    persist_resilience_event(db_path, "restore_started", None, &note)?;
                }
                let event = format!(
                    "verify_restore snapshot={} artifact={}",
                    selected.snapshot_id,
                    target_path.display()
                );
                control.runtime.mark_recovery_event(event.clone());
                if let Some(duration_ms) = control.resilience.complete_restore() {
                    persist_resilience_event(
                        db_path,
                        "restore_completed",
                        Some(duration_ms),
                        "verify_restore_completed",
                    )?;
                }
                persist_runtime_marker(db_path, "last_recovery_event", &event)?;
            }

            let keep_artifact = input.keep_artifact;
            if !keep_artifact {
                let _ = std::fs::remove_file(&target_path);
                let _ = std::fs::remove_file(Path::new(&format!("{}-wal", target_path.display())));
                let _ = std::fs::remove_file(Path::new(&format!("{}-shm", target_path.display())));
            }
            let payload = json!({
                "selected_snapshot": selected,
                "restore_verified": report.integrity_check_ok,
                "verification": report,
                "restore_artifact_path": target_path.display().to_string(),
                "artifact_kept": keep_artifact,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/recovery/prune-snapshots") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let input = parse_optional_json_body::<RecoveryPruneRequest>(request)
                .map_err(std::io::Error::other)?;
            let backup_dir;
            let retention_seconds;
            {
                let control = state
                    .lock()
                    .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
                backup_dir = control.profile.recovery.backup_dir.clone();
                retention_seconds = input
                    .retention_seconds
                    .unwrap_or(control.profile.recovery.pitr_retention_seconds);
            }
            let report = prune_backup_snapshots(&backup_dir, retention_seconds)?;
            let payload = json!({
                "backup_dir": backup_dir,
                "report": report,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/control/v1/chaos/status") => {
            let control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let payload = json!({
                "chaos": control.chaos.to_json(),
                "resilience": control.resilience.to_json(control.chaos.active_count()),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/chaos/inject") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let inject = match parse_json_body::<ChaosInjectRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let scenario = inject.scenario.as_str().to_string();
            control.chaos.inject(inject);
            control.resilience.chaos_injections_total =
                control.resilience.chaos_injections_total.saturating_add(1);
            persist_chaos_event(db_path, "inject", &scenario)?;
            let payload = json!({
                "status": "injected",
                "scenario": scenario,
                "chaos": control.chaos.to_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/chaos/clear") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let clear = parse_optional_json_body::<ChaosClearRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let scenario_name = clear.scenario.map(|scenario| scenario.as_str().to_string());
            control.chaos.clear(clear.scenario);
            persist_chaos_event(db_path, "clear", scenario_name.as_deref().unwrap_or("all"))?;
            let payload = json!({
                "status": "cleared",
                "scenario": scenario_name,
                "chaos": control.chaos.to_json(),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/replication/append") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let append = match parse_json_body::<ReplicationAppendRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            if !control.profile.replication.enabled || control.runtime.role != ServerRole::Primary {
                return Ok((
                    409,
                    "application/json",
                    json!({"error": "replication append requires enabled primary role"})
                        .to_string(),
                ));
            }

            control.runtime.current_term = control.runtime.current_term.max(1);
            let operation = append
                .operation
                .unwrap_or_else(|| "state_mutation".to_string())
                .trim()
                .to_string();
            if operation.is_empty() {
                return Ok((
                    400,
                    "application/json",
                    json!({"error": "operation cannot be empty"}).to_string(),
                ));
            }

            let current_term = control.runtime.current_term;
            let node_id = control.profile.replication.node_id.clone();
            let entry = match control.replication_log.append_leader_event(
                current_term,
                &node_id,
                operation,
                append.payload,
                &node_id,
            ) {
                Ok(entry) => entry,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };
            let last_index = control.replication_log.last_index();
            let last_term = control.replication_log.last_term();
            control.runtime.note_log_position(last_index, last_term);

            let new_commit = control.replication_log.compute_commit_index(
                control.runtime.commit_index,
                control.profile.replication.sync_ack_quorum,
            );
            control.runtime.advance_commit_index(new_commit);

            append_replication_entry_to_store(
                db_path,
                &entry,
                entry.index <= control.runtime.commit_index,
            )?;
            if new_commit > 0 {
                mark_committed_replication_entries(db_path, new_commit)?;
            }

            let payload = json!({
                "entry": entry,
                "commit_index": control.runtime.commit_index,
                "required_quorum": control.profile.replication.sync_ack_quorum,
                "ack_count": control.replication_log.ack_count(control.runtime.last_log_index),
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/replication/receive") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }

            let receive = match parse_json_body::<ReplicationReceiveRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            if !control.profile.replication.enabled {
                return Ok((
                    409,
                    "application/json",
                    json!({"error": "replication is not enabled"}).to_string(),
                ));
            }
            if receive.term < control.runtime.current_term {
                return Ok((
                    409,
                    "application/json",
                    json!({"error": "stale replication term", "term": control.runtime.current_term}).to_string(),
                ));
            }

            control.runtime.adopt_term(receive.term);
            control
                .runtime
                .step_down_to_replica(Some(receive.leader_id.clone()));

            if let Err(error) = control.replication_log.append_remote_entries(
                receive.prev_log_index,
                receive.prev_log_term,
                &receive.entries,
            ) {
                return Ok((409, "application/json", json!({"error": error}).to_string()));
            }

            let last_index = control.replication_log.last_index();
            let last_term = control.replication_log.last_term();
            control.runtime.note_log_position(last_index, last_term);
            if let Some(leader_commit) = receive.leader_commit {
                control.runtime.advance_commit_index(leader_commit);
            }

            rewrite_replication_log_store(
                db_path,
                &control.replication_log,
                control.runtime.commit_index,
            )?;
            let payload = json!({
                "accepted": true,
                "term": control.runtime.current_term,
                "last_log_index": control.runtime.last_log_index,
                "last_log_term": control.runtime.last_log_term,
                "commit_index": control.runtime.commit_index,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/replication/ack") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let ack = match parse_json_body::<ReplicationAckRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            if ack.index == 0 {
                return Ok((
                    400,
                    "application/json",
                    json!({"error": "ack index must be >= 1"}).to_string(),
                ));
            }
            let ack_count = control
                .replication_log
                .acknowledge(ack.index, ack.node_id.clone());
            if ack_count == 0 {
                return Ok((
                    409,
                    "application/json",
                    json!({"error": "ack index is outside local replication log"}).to_string(),
                ));
            }
            control
                .replica_progress
                .insert(ack.node_id.clone(), ack.index);

            let new_commit = control.replication_log.compute_commit_index(
                control.runtime.commit_index,
                control.profile.replication.sync_ack_quorum,
            );
            control.runtime.advance_commit_index(new_commit);
            if new_commit > 0 {
                mark_committed_replication_entries(db_path, new_commit)?;
            }

            let payload = json!({
                "node_id": ack.node_id,
                "index": ack.index,
                "ack_count": ack_count,
                "commit_index": control.runtime.commit_index,
                "required_quorum": control.profile.replication.sync_ack_quorum,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/replication/reconcile") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let reconcile = parse_optional_json_body::<ReplicationReconcileRequest>(request)
                .map_err(std::io::Error::other)?;
            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            let node_id = reconcile
                .node_id
                .unwrap_or_else(|| "unknown-replica".to_string());
            let last_applied = reconcile.last_applied_index.unwrap_or(0);
            control
                .replica_progress
                .insert(node_id.clone(), last_applied);

            if let Some(peer_commit) = reconcile.commit_index {
                // Replica reconciliation reports can only move commit forward up to local log head.
                control.runtime.advance_commit_index(peer_commit);
            }
            if let Some(lag_ms) = reconcile.replication_lag_ms {
                control.runtime.replication_lag_ms = lag_ms;
            } else {
                let lag_entries = control.runtime.commit_index.saturating_sub(last_applied);
                control.runtime.replication_lag_ms =
                    lag_entries.saturating_mul(control.profile.replication.heartbeat_interval_ms);
            }

            let missing_from = last_applied.saturating_add(1);
            let missing_entries = control.replication_log.entries_from(missing_from, 128);
            persist_reconcile_event(
                db_path,
                &node_id,
                last_applied,
                control.runtime.commit_index,
                control.runtime.replication_lag_ms,
            )?;

            let payload = json!({
                "node_id": node_id,
                "last_applied_index": last_applied,
                "commit_index": control.runtime.commit_index,
                "missing_entries_count": missing_entries.len(),
                "missing_entries": missing_entries,
                "replication_lag_ms": control.runtime.replication_lag_ms,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/election/request-vote") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let vote = match parse_json_body::<ElectionVoteRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            if !control.profile.replication.enabled {
                return Ok((
                    409,
                    "application/json",
                    json!({"error": "election requires replication mode"}).to_string(),
                ));
            }

            let mut granted = false;
            let reason: String;
            if vote.term < control.runtime.current_term {
                reason = "stale_term".to_string();
            } else {
                control.runtime.adopt_term(vote.term);
                if control.runtime.can_grant_vote(
                    vote.term,
                    &vote.candidate_id,
                    vote.candidate_last_log_index,
                    vote.candidate_last_log_term,
                ) {
                    granted = true;
                    control
                        .runtime
                        .grant_vote(vote.term, vote.candidate_id.clone());
                    reason = "vote_granted".to_string();
                } else if control
                    .runtime
                    .voted_for
                    .as_ref()
                    .is_some_and(|value| value != &vote.candidate_id)
                {
                    reason = "already_voted".to_string();
                } else {
                    reason = "candidate_log_outdated".to_string();
                }
            }

            persist_vote_event(
                db_path,
                control.runtime.current_term,
                &control.profile.replication.node_id,
                &vote.candidate_id,
                granted,
                &reason,
            )?;

            let payload = json!({
                "term": control.runtime.current_term,
                "vote_granted": granted,
                "reason": reason,
                "voted_for": control.runtime.voted_for,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("POST", "/control/v1/election/heartbeat") => {
            if !authorize_control_request(request, config) {
                return Ok(unauthorized_response());
            }
            let heartbeat = match parse_json_body::<ElectionHeartbeatRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    return Ok((400, "application/json", json!({"error": error}).to_string()));
                }
            };

            let mut control = state
                .lock()
                .map_err(|_| std::io::Error::other("control-plane state lock poisoned"))?;
            if heartbeat.term < control.runtime.current_term {
                return Ok((
                    409,
                    "application/json",
                    json!({"accepted": false, "reason": "stale_term", "term": control.runtime.current_term}).to_string(),
                ));
            }

            control.runtime.adopt_term(heartbeat.term);
            if heartbeat.leader_id != control.profile.replication.node_id {
                control
                    .runtime
                    .step_down_to_replica(Some(heartbeat.leader_id.clone()));
            }
            if let Some(leader_last_log_index) = heartbeat.leader_last_log_index
                && leader_last_log_index > control.runtime.last_log_index
            {
                control.runtime.mark_failover_started();
            }
            control.runtime.mark_heartbeat(
                Some(heartbeat.leader_id.clone()),
                heartbeat.commit_index,
                heartbeat.replication_lag_ms.unwrap_or(0),
            );
            if control.runtime.failover_in_progress
                && let Some(duration_ms) = control.resilience.complete_failover()
            {
                persist_resilience_event(
                    db_path,
                    "failover_completed",
                    Some(duration_ms),
                    "heartbeat_stabilized",
                )?;
            }
            if control.runtime.commit_index > 0 {
                mark_committed_replication_entries(db_path, control.runtime.commit_index)?;
            }

            let payload = json!({
                "accepted": true,
                "term": control.runtime.current_term,
                "leader_id": heartbeat.leader_id,
                "role": control.runtime.role,
                "commit_index": control.runtime.commit_index,
                "last_applied_index": control.runtime.last_applied_index,
            });
            Ok((200, "application/json", payload.to_string()))
        }
        ("GET", "/v1/openapi.json") => Ok((
            200,
            "application/json",
            openapi_query_document(config.enable_sql_endpoint).to_string(),
        )),
        ("POST", "/v1/sql") if config.enable_sql_endpoint => {
            let sql_request = match parse_json_body::<SqlApiRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            execute_sql_api_statement(db_path, sql_profile, config, request, sql_request)
        }
        ("POST", "/v1/query") if config.enable_sql_endpoint => {
            let input = match parse_json_body::<QueryApiRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            match execute_query_api(db, config, request, path, input, false) {
                Ok(payload) => Ok((200, "application/json", payload.to_string())),
                Err(error)
                    if matches!(
                        &error,
                        crate::SqlRiteError::Io(io_error)
                            if io_error.kind() == std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    Ok((
                        403,
                        "application/json",
                        json!({"error": error.to_string()}).to_string(),
                    ))
                }
                Err(error) => Ok((
                    400,
                    "application/json",
                    json!({"error": error.to_string()}).to_string(),
                )),
            }
        }
        ("POST", "/v1/query-compact") if config.enable_sql_endpoint => {
            let input = match parse_json_body::<QueryApiRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            match execute_query_api(db, config, request, path, input, true) {
                Ok(payload) => Ok((200, "application/json", payload.to_string())),
                Err(error)
                    if matches!(
                        &error,
                        crate::SqlRiteError::Io(io_error)
                            if io_error.kind() == std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    Ok((
                        403,
                        "application/json",
                        json!({"error": error.to_string()}).to_string(),
                    ))
                }
                Err(error) => Ok((
                    400,
                    "application/json",
                    json!({"error": error.to_string()}).to_string(),
                )),
            }
        }
        ("POST", "/v1/rerank-hook") if config.enable_sql_endpoint => {
            let input = match parse_json_body::<RerankHookRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            match execute_rerank_hook_api(db, config, request, path, input) {
                Ok(payload) => Ok((200, "application/json", payload.to_string())),
                Err(error)
                    if matches!(
                        &error,
                        crate::SqlRiteError::Io(io_error)
                            if io_error.kind() == std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    Ok((
                        403,
                        "application/json",
                        json!({"error": error.to_string()}).to_string(),
                    ))
                }
                Err(error) => Ok((
                    400,
                    "application/json",
                    json!({"error": error.to_string()}).to_string(),
                )),
            }
        }
        ("POST", "/grpc/sqlrite.v1.QueryService/Sql") if config.enable_sql_endpoint => {
            let sql_request = match parse_json_body::<SqlApiRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            execute_sql_api_statement(db_path, sql_profile, config, request, sql_request)
        }
        ("POST", "/grpc/sqlrite.v1.QueryService/Query") if config.enable_sql_endpoint => {
            let input = match parse_json_body::<QueryApiRequest>(request) {
                Ok(payload) => payload,
                Err(error) => {
                    let payload = json!({"error": error});
                    return Ok((400, "application/json", payload.to_string()));
                }
            };
            match execute_query_api(db, config, request, path, input, false) {
                Ok(payload) => Ok((200, "application/json", payload.to_string())),
                Err(error)
                    if matches!(
                        &error,
                        crate::SqlRiteError::Io(io_error)
                            if io_error.kind() == std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    Ok((
                        403,
                        "application/json",
                        json!({"error": error.to_string()}).to_string(),
                    ))
                }
                Err(error) => Ok((
                    400,
                    "application/json",
                    json!({"error": error.to_string()}).to_string(),
                )),
            }
        }
        ("POST", _) if path.starts_with("/control/") => Ok((
            404,
            "application/json",
            json!({"error": "unknown control-plane endpoint"}).to_string(),
        )),
        ("GET", _) if path.starts_with("/control/") => Ok((
            404,
            "application/json",
            json!({"error": "unknown control-plane endpoint"}).to_string(),
        )),
        (method, "/v1/sql") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /v1/sql"}).to_string(),
        )),
        (method, "/v1/query") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /v1/query"}).to_string(),
        )),
        (method, "/v1/query-compact") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /v1/query-compact"}).to_string(),
        )),
        (method, "/v1/rerank-hook") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /v1/rerank-hook"}).to_string(),
        )),
        (method, "/grpc/sqlrite.v1.QueryService/Sql") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /grpc/sqlrite.v1.QueryService/Sql"})
                .to_string(),
        )),
        (method, "/grpc/sqlrite.v1.QueryService/Query") if method != "POST" => Ok((
            405,
            "application/json",
            json!({"error": "method not allowed; use POST /grpc/sqlrite.v1.QueryService/Query"})
                .to_string(),
        )),
        _ => Ok((404, "text/plain; charset=utf-8", "not found".to_string())),
    }
}

fn execute_sql_api_statement(
    db_path: &Path,
    sql_profile: DurabilityProfile,
    config: &ServerConfig,
    request: &HttpRequest,
    input: SqlApiRequest,
) -> Result<(u16, &'static str, String)> {
    if let Err(response) = authorize_request(config, request, AccessOperation::SqlAdmin, "/v1/sql")
    {
        return Ok(response);
    }

    let statement_len = input.statement.len();
    match execute_sdk_sql(db_path, sql_profile, input) {
        Ok(payload) => {
            audit_request(
                config,
                request,
                AccessOperation::SqlAdmin,
                true,
                json!({"path": request.path.as_str(), "statement_len": statement_len}),
            )?;
            Ok((200, "application/json", payload.to_string()))
        }
        Err(error) if error.is_validation() => Ok((
            400,
            "application/json",
            json!({"error": error.to_string()}).to_string(),
        )),
        Err(error) => Ok((
            500,
            "application/json",
            json!({"error": error.to_string()}).to_string(),
        )),
    }
}

fn execute_query_api(
    db: &SqlRite,
    config: &ServerConfig,
    request: &HttpRequest,
    path: &str,
    mut input: QueryApiRequest,
    compact: bool,
) -> Result<Value> {
    let context = authorize_request(config, request, AccessOperation::Query, path).map_err(
        |(_, _, body)| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                extract_error_message(&body),
            )
        },
    )?;

    if let Some(context) = context {
        let tenant = context.tenant_id.clone();
        let filters = input.metadata_filters.get_or_insert_with(HashMap::new);
        if let Some(existing) = filters.get("tenant")
            && existing != &tenant
        {
            audit_request(
                config,
                request,
                AccessOperation::Query,
                false,
                json!({"path": path, "reason": "tenant filter mismatch"}),
            )?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "tenant filter mismatch",
            )
            .into());
        }
        filters.insert("tenant".to_string(), tenant);
    }

    let envelope = execute_sdk_query(db, input).map_err(std::io::Error::other)?;
    audit_request(
        config,
        request,
        AccessOperation::Query,
        true,
        json!({"path": path, "row_count": envelope.row_count, "compact": compact}),
    )?;
    if compact {
        Ok(compact_query_envelope(envelope))
    } else {
        Ok(serde_json::to_value(envelope)?)
    }
}

fn compact_query_envelope(envelope: sqlrite_sdk_core::QueryEnvelope<crate::SearchResult>) -> Value {
    let mut chunk_ids = Vec::with_capacity(envelope.rows.len());
    let mut hybrid_scores = Vec::with_capacity(envelope.rows.len());
    let mut vector_scores = Vec::with_capacity(envelope.rows.len());
    let mut text_scores = Vec::with_capacity(envelope.rows.len());
    let include_doc_ids = envelope.rows.iter().any(|row| !row.doc_id.is_empty());
    let include_contents = envelope.rows.iter().any(|row| !row.content.is_empty());
    let include_metadata = envelope.rows.iter().any(|row| !row.metadata.is_null());
    let mut doc_ids = if include_doc_ids {
        Some(Vec::with_capacity(envelope.rows.len()))
    } else {
        None
    };
    let mut contents = if include_contents {
        Some(Vec::with_capacity(envelope.rows.len()))
    } else {
        None
    };
    let mut metadata = if include_metadata {
        Some(Vec::with_capacity(envelope.rows.len()))
    } else {
        None
    };

    for row in envelope.rows {
        chunk_ids.push(row.chunk_id);
        hybrid_scores.push(row.hybrid_score);
        vector_scores.push(row.vector_score);
        text_scores.push(row.text_score);
        if let Some(doc_ids) = &mut doc_ids {
            doc_ids.push(row.doc_id);
        }
        if let Some(contents) = &mut contents {
            contents.push(row.content);
        }
        if let Some(metadata_rows) = &mut metadata {
            metadata_rows.push(row.metadata);
        }
    }

    let mut payload = serde_json::Map::new();
    payload.insert("kind".to_string(), json!("query_compact"));
    payload.insert("row_count".to_string(), json!(chunk_ids.len()));
    payload.insert("chunk_ids".to_string(), json!(chunk_ids));
    payload.insert("hybrid_scores".to_string(), json!(hybrid_scores));
    payload.insert("vector_scores".to_string(), json!(vector_scores));
    payload.insert("text_scores".to_string(), json!(text_scores));
    if let Some(doc_ids) = doc_ids {
        payload.insert("doc_ids".to_string(), json!(doc_ids));
    }
    if let Some(contents) = contents {
        payload.insert("contents".to_string(), json!(contents));
    }
    if let Some(metadata_rows) = metadata {
        payload.insert("metadata".to_string(), json!(metadata_rows));
    }
    Value::Object(payload)
}

fn execute_rerank_hook_api(
    db: &SqlRite,
    config: &ServerConfig,
    request: &HttpRequest,
    path: &str,
    input: RerankHookRequest,
) -> Result<Value> {
    let context = authorize_request(config, request, AccessOperation::Query, path).map_err(
        |(_, _, body)| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                extract_error_message(&body),
            )
        },
    )?;

    let candidate_count = input.candidate_count.unwrap_or(25).max(1);
    let mut metadata_filters = input.metadata_filters.unwrap_or_default();
    if let Some(context) = context {
        if let Some(existing) = metadata_filters.get("tenant")
            && existing != &context.tenant_id
        {
            audit_request(
                config,
                request,
                AccessOperation::Query,
                false,
                json!({"path": path, "reason": "tenant filter mismatch"}),
            )?;
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "tenant filter mismatch",
            )
            .into());
        }
        metadata_filters.insert("tenant".to_string(), context.tenant_id);
    }

    let request_model = crate::SearchRequest {
        query_text: input.query_text,
        query_embedding: input.query_embedding,
        top_k: candidate_count,
        alpha: input.alpha.unwrap_or(sqlrite_sdk_core::DEFAULT_ALPHA),
        candidate_limit: input
            .candidate_limit
            .unwrap_or(sqlrite_sdk_core::DEFAULT_CANDIDATE_LIMIT)
            .max(candidate_count),
        query_profile: parse_query_profile_api(input.query_profile.as_deref())
            .map_err(SqlRiteError::InvalidBenchmarkConfig)?,
        metadata_filters,
        doc_id: input.doc_id,
        ..crate::SearchRequest::default()
    };
    request_model
        .validate()
        .map_err(|error| SqlRiteError::InvalidBenchmarkConfig(error.to_string()))?;
    let rows = db.search(request_model)?;
    audit_request(
        config,
        request,
        AccessOperation::Query,
        true,
        json!({"path": path, "row_count": rows.len(), "kind": "rerank_hook"}),
    )?;
    Ok(json!({
        "kind": "rerank_hook",
        "row_count": rows.len(),
        "rows": rows,
    }))
}

fn parse_query_profile_api(value: Option<&str>) -> std::result::Result<QueryProfile, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(QueryProfile::Balanced),
        Some("balanced") => Ok(QueryProfile::Balanced),
        Some("latency") => Ok(QueryProfile::Latency),
        Some("recall") => Ok(QueryProfile::Recall),
        Some(other) => Err(format!(
            "invalid query_profile `{other}`; expected balanced|latency|recall"
        )),
    }
}

fn openapi_query_document(sql_enabled: bool) -> Value {
    let mut paths = serde_json::Map::new();
    if sql_enabled {
        paths.insert(
            "/v1/sql".to_string(),
            json!({
                "post": {
                    "summary": "Execute retrieval SQL statement",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SqlRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Statement executed"},
                        "400": {"description": "Invalid SQL request"}
                    }
                }
            }),
        );
        paths.insert(
            "/v1/query".to_string(),
            json!({
                "post": {
                    "summary": "Run semantic/lexical/hybrid retrieval query",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/QueryRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Query results"},
                        "400": {"description": "Invalid query request"}
                    }
                }
            }),
        );
        paths.insert(
            "/v1/query-compact".to_string(),
            json!({
                "post": {
                    "summary": "Run retrieval query with compact array-oriented response for agents and benchmarks",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/QueryRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Compact query results"},
                        "400": {"description": "Invalid query request"}
                    }
                }
            }),
        );
        paths.insert(
            "/v1/rerank-hook".to_string(),
            json!({
                "post": {
                    "summary": "Produce scored candidate features for external rerankers",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/RerankHookRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Rerank candidate payload"},
                        "400": {"description": "Invalid rerank hook request"}
                    }
                }
            }),
        );
        paths.insert(
            "/grpc/sqlrite.v1.QueryService/Sql".to_string(),
            json!({
                "post": {
                    "summary": "gRPC-compat SQL query method over HTTP JSON bridge",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/SqlRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Statement executed"},
                        "400": {"description": "Invalid SQL request"}
                    }
                }
            }),
        );
        paths.insert(
            "/grpc/sqlrite.v1.QueryService/Query".to_string(),
            json!({
                "post": {
                    "summary": "gRPC-compat retrieval query method over HTTP JSON bridge",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/QueryRequest"}
                            }
                        }
                    },
                    "responses": {
                        "200": {"description": "Query results"},
                        "400": {"description": "Invalid query request"}
                    }
                }
            }),
        );
    }
    paths.insert(
        "/v1/openapi.json".to_string(),
        json!({
            "get": {
                "summary": "Fetch OpenAPI document for query surfaces",
                "responses": {
                    "200": {"description": "OpenAPI document"}
                }
            }
        }),
    );

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "SQLRite Query API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "OpenAPI baseline for SQL and retrieval query surfaces."
        },
        "paths": paths,
        "components": {
            "schemas": {
                "SqlRequest": {
                    "type": "object",
                    "required": ["statement"],
                    "properties": {
                        "statement": {"type": "string"}
                    }
                },
                "QueryRequest": {
                    "type": "object",
                    "properties": {
                        "query_text": {"type": "string"},
                        "query_embedding": {
                            "type": "array",
                            "items": {"type": "number"}
                        },
                        "top_k": {"type": "integer", "minimum": 1},
                        "alpha": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "candidate_limit": {"type": "integer", "minimum": 1},
                        "include_payloads": {"type": "boolean"},
                        "query_profile": {
                            "type": "string",
                            "enum": ["balanced", "latency", "recall"]
                        },
                        "metadata_filters": {
                            "type": "object",
                            "additionalProperties": {"type": "string"}
                        },
                        "doc_id": {"type": "string"}
                    }
                },
                "RerankHookRequest": {
                    "type": "object",
                    "properties": {
                        "query_text": {"type": "string"},
                        "query_embedding": {
                            "type": "array",
                            "items": {"type": "number"}
                        },
                        "candidate_count": {"type": "integer", "minimum": 1},
                        "alpha": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "candidate_limit": {"type": "integer", "minimum": 1},
                        "query_profile": {
                            "type": "string",
                            "enum": ["balanced", "latency", "recall"]
                        },
                        "metadata_filters": {
                            "type": "object",
                            "additionalProperties": {"type": "string"}
                        },
                        "doc_id": {"type": "string"}
                    }
                }
            }
        }
    })
}

fn split_path_and_query(path: &str) -> (&str, HashMap<String, String>) {
    let mut parts = path.splitn(2, '?');
    let raw_path = parts.next().unwrap_or("/");
    let query = parts.next().unwrap_or_default();
    (raw_path, parse_query_params(query))
}

fn parse_query_params(raw: &str) -> HashMap<String, String> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            if key.is_empty() {
                return None;
            }
            let value = parts.next().unwrap_or_default().trim();
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

fn authorize_control_request(request: &HttpRequest, config: &ServerConfig) -> bool {
    let Some(expected) = &config.control_api_token else {
        return true;
    };

    request
        .headers
        .get("x-sqlrite-control-token")
        .is_some_and(|provided| provided == expected)
}

fn unauthorized_response() -> (u16, &'static str, String) {
    (
        401,
        "application/json",
        json!({"error": "unauthorized control-plane request"}).to_string(),
    )
}

fn authorize_request(
    config: &ServerConfig,
    request: &HttpRequest,
    operation: AccessOperation,
    path: &str,
) -> std::result::Result<Option<AccessContext>, (u16, &'static str, String)> {
    if !config.security.enabled() {
        return Ok(None);
    }

    let Some(context) = extract_access_context(request, config) else {
        let response = (
            401,
            "application/json",
            json!({"error": "missing auth context headers"}).to_string(),
        );
        let _ = audit_request(
            config,
            request,
            operation,
            false,
            json!({"path": path, "reason": "missing_auth_context"}),
        );
        return Err(response);
    };

    if let Some(policy) = &config.security.policy
        && let Err(error) = policy.authorize(&context, operation, &context.tenant_id)
    {
        let response = (
            403,
            "application/json",
            json!({"error": error.to_string()}).to_string(),
        );
        let _ = audit_request(
            config,
            request,
            operation,
            false,
            json!({"path": path, "reason": error.to_string()}),
        );
        return Err(response);
    }

    Ok(Some(context))
}

fn extract_access_context(request: &HttpRequest, config: &ServerConfig) -> Option<AccessContext> {
    let actor_id = request.headers.get("x-sqlrite-actor-id").cloned();
    let tenant_id = request.headers.get("x-sqlrite-tenant-id").cloned();
    let roles = request
        .headers
        .get("x-sqlrite-roles")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    match (actor_id, tenant_id) {
        (Some(actor_id), Some(tenant_id)) => {
            Some(AccessContext::new(actor_id, tenant_id).with_roles(roles))
        }
        _ if config.security.require_auth_context || config.security.enabled() => None,
        _ => None,
    }
}

fn audit_request(
    config: &ServerConfig,
    request: &HttpRequest,
    operation: AccessOperation,
    allowed: bool,
    detail: Value,
) -> Result<()> {
    let Some(path) = &config.security.audit_log_path else {
        return Ok(());
    };

    let context = extract_access_context(request, config)
        .unwrap_or_else(|| AccessContext::new("anonymous", "unknown"));
    let logger = JsonlAuditLogger::new(path, config.security.audit_redacted_fields.clone())?;
    logger.log(&AuditEvent {
        unix_ms: unix_ms_now(),
        actor_id: context.actor_id,
        tenant_id: context.tenant_id,
        operation,
        allowed,
        detail,
    })
}

fn security_summary_json(config: &ServerConfig) -> Value {
    json!({
        "enabled": config.security.enabled(),
        "secure_defaults": config.security.secure_defaults,
        "require_auth_context": config.security.require_auth_context,
        "audit_log_path": config.security.audit_log_path,
        "rbac_roles": config.security.policy.as_ref().map(|policy| policy.role_names()).unwrap_or_default(),
    })
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.to_string())
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(
    request: &HttpRequest,
) -> std::result::Result<T, String> {
    if request.body.is_empty() {
        return Err("JSON body is required".to_string());
    }
    serde_json::from_slice::<T>(&request.body)
        .map_err(|error| format!("invalid JSON body: {error}"))
}

fn parse_optional_json_body<T>(request: &HttpRequest) -> std::result::Result<T, String>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if request.body.is_empty() {
        return Ok(T::default());
    }
    parse_json_body::<T>(request)
}

#[derive(Debug, serde::Serialize)]
struct AutoFailoverEvent {
    promoted: bool,
    reason: String,
    term: u64,
    leader_id: String,
    failover_duration_ms: Option<u64>,
}

#[derive(Debug)]
struct AutoFailoverEvalInput {
    force: bool,
    simulated_elapsed_ms: Option<u64>,
    reason: Option<String>,
}

fn maybe_trigger_automatic_failover(
    db_path: &Path,
    control: &mut ControlPlaneState,
    input: AutoFailoverEvalInput,
) -> Result<Option<AutoFailoverEvent>> {
    if !control.profile.replication.enabled {
        return Ok(None);
    }
    if control.profile.replication.failover_mode != FailoverMode::Automatic {
        return Ok(None);
    }
    if control.runtime.role == ServerRole::Primary {
        return Ok(None);
    }

    let heartbeat_elapsed_ms = if let Some(simulated) = input.simulated_elapsed_ms {
        simulated
    } else if let Some(last) = control.runtime.last_heartbeat_unix_ms {
        unix_ms_now().saturating_sub(last)
    } else if input.force {
        control
            .profile
            .replication
            .election_timeout_ms
            .saturating_add(1)
    } else {
        return Ok(None);
    };
    let timed_out = heartbeat_elapsed_ms >= control.profile.replication.election_timeout_ms;
    if !input.force && !timed_out {
        return Ok(None);
    }

    let reason = input
        .reason
        .unwrap_or_else(|| "automatic_failover_timeout".to_string());
    control.runtime.mark_failover_started();
    control.resilience.start_failover();
    persist_resilience_event(db_path, "failover_started", None, &reason)?;

    let node_id = control.profile.replication.node_id.clone();
    control.profile.replication.role = ServerRole::Primary;
    control.runtime.promote_to_primary(node_id.clone());
    let duration = control.resilience.complete_failover();
    if let Some(duration_ms) = duration {
        persist_resilience_event(
            db_path,
            "failover_completed",
            Some(duration_ms),
            "automatic_failover_promote",
        )?;
    }
    persist_runtime_marker(
        db_path,
        "last_role_transition",
        &serde_json::to_string(&control.runtime)?,
    )?;

    let entry = control
        .replication_log
        .append_leader_event(
            control.runtime.current_term.max(1),
            &node_id,
            "automatic_failover_promote".to_string(),
            json!({
                "reason": reason,
                "heartbeat_elapsed_ms": heartbeat_elapsed_ms,
                "election_timeout_ms": control.profile.replication.election_timeout_ms,
            }),
            &node_id,
        )
        .map_err(std::io::Error::other)?;
    let last_index = control.replication_log.last_index();
    let last_term = control.replication_log.last_term();
    control.runtime.note_log_position(last_index, last_term);
    let new_commit = control.replication_log.compute_commit_index(
        control.runtime.commit_index,
        control.profile.replication.sync_ack_quorum,
    );
    control.runtime.advance_commit_index(new_commit);
    append_replication_entry_to_store(
        db_path,
        &entry,
        entry.index <= control.runtime.commit_index,
    )?;
    if new_commit > 0 {
        mark_committed_replication_entries(db_path, new_commit)?;
    }

    Ok(Some(AutoFailoverEvent {
        promoted: true,
        reason,
        term: control.runtime.current_term,
        leader_id: node_id,
        failover_duration_ms: duration,
    }))
}

fn chaos_blocking_response(
    control: &mut ControlPlaneState,
    request: &HttpRequest,
    path: &str,
) -> Option<(u16, &'static str, String)> {
    if control.chaos.has(ChaosScenario::NodeCrash) {
        control.resilience.chaos_blocked_requests_total = control
            .resilience
            .chaos_blocked_requests_total
            .saturating_add(1);
        return Some((
            503,
            "application/json",
            json!({
                "error": "chaos fault active: node_crash",
                "path": path,
            })
            .to_string(),
        ));
    }

    if control.chaos.has(ChaosScenario::DiskFull) {
        let blocks = path == "/control/v1/replication/append"
            || path == "/v1/sql"
            || path == "/grpc/sqlrite.v1.QueryService/Sql"
            || path == "/control/v1/recovery/start"
            || path == "/control/v1/recovery/mark-restored"
            || path == "/control/v1/recovery/snapshot"
            || path == "/control/v1/recovery/verify-restore"
            || path == "/control/v1/recovery/prune-snapshots";
        if blocks && request.method == "POST" {
            control.resilience.chaos_blocked_requests_total = control
                .resilience
                .chaos_blocked_requests_total
                .saturating_add(1);
            return Some((
                507,
                "application/json",
                json!({
                    "error": "chaos fault active: disk_full",
                    "path": path,
                })
                .to_string(),
            ));
        }
    }

    if control.chaos.has(ChaosScenario::PartitionSubset)
        && matches!(
            path,
            "/control/v1/election/heartbeat" | "/control/v1/replication/receive"
        )
    {
        control.resilience.chaos_blocked_requests_total = control
            .resilience
            .chaos_blocked_requests_total
            .saturating_add(1);
        return Some((
            503,
            "application/json",
            json!({
                "error": "chaos fault active: partition_subset",
                "path": path,
            })
            .to_string(),
        ));
    }

    None
}

fn restore_replication_state(db_path: &Path, control: &mut ControlPlaneState) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;

    let mut stmt = conn.prepare(
        "
        SELECT idx, term, leader_id, operation, payload_json, checksum, created_at_unix_ms, committed
        FROM replication_log
        ORDER BY idx ASC
        ",
    )?;
    let mut rows = stmt.query([])?;
    let mut entries = Vec::new();
    let mut commit_index = 0u64;

    while let Some(row) = rows.next()? {
        let index: u64 = row.get(0)?;
        let term: u64 = row.get(1)?;
        let leader_id: String = row.get(2)?;
        let operation: String = row.get(3)?;
        let payload_json: String = row.get(4)?;
        let checksum: String = row.get(5)?;
        let created_at_unix_ms: u64 = row.get(6)?;
        let committed: i64 = row.get(7)?;
        let payload = serde_json::from_str::<Value>(&payload_json)?;
        entries.push(ReplicationLogEntry {
            index,
            term,
            leader_id,
            operation,
            payload,
            checksum,
            created_at_unix_ms,
        });
        if committed == 1 {
            commit_index = index;
        }
    }

    control.replication_log =
        ReplicationLog::from_entries(entries).map_err(std::io::Error::other)?;
    control.runtime.note_log_position(
        control.replication_log.last_index(),
        control.replication_log.last_term(),
    );
    control.runtime.advance_commit_index(commit_index);
    control.runtime.current_term = control
        .runtime
        .current_term
        .max(control.runtime.last_log_term);

    let voted_for: Option<String> = conn
        .query_row(
            "
            SELECT candidate_id
            FROM election_votes
            WHERE term = ?1 AND voter_id = ?2 AND granted = 1
            ORDER BY rowid DESC
            LIMIT 1
            ",
            params![
                control.runtime.current_term,
                control.profile.replication.node_id.as_str()
            ],
            |row| row.get(0),
        )
        .optional()?;
    control.runtime.voted_for = voted_for;

    control.resilience.failover_events_total = conn.query_row(
        "SELECT COUNT(*) FROM ha_resilience_events WHERE event_type = 'failover_started'",
        [],
        |row| row.get(0),
    )?;
    control.resilience.failover_completed_total = conn.query_row(
        "SELECT COUNT(*) FROM ha_resilience_events WHERE event_type = 'failover_completed'",
        [],
        |row| row.get(0),
    )?;
    control.resilience.cumulative_failover_duration_ms = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_ms), 0) FROM ha_resilience_events WHERE event_type = 'failover_completed'",
            [],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as u128;
    control.resilience.last_failover_duration_ms = conn
        .query_row(
            "SELECT duration_ms FROM ha_resilience_events WHERE event_type = 'failover_completed' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| value.max(0) as u64);

    control.resilience.restore_events_total = conn.query_row(
        "SELECT COUNT(*) FROM ha_resilience_events WHERE event_type = 'restore_started'",
        [],
        |row| row.get(0),
    )?;
    control.resilience.restore_completed_total = conn.query_row(
        "SELECT COUNT(*) FROM ha_resilience_events WHERE event_type = 'restore_completed'",
        [],
        |row| row.get(0),
    )?;
    control.resilience.cumulative_restore_duration_ms = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_ms), 0) FROM ha_resilience_events WHERE event_type = 'restore_completed'",
            [],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as u128;
    control.resilience.last_restore_duration_ms = conn
        .query_row(
            "SELECT duration_ms FROM ha_resilience_events WHERE event_type = 'restore_completed' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|value| value.max(0) as u64);

    control.resilience.chaos_injections_total = conn.query_row(
        "SELECT COUNT(*) FROM ha_chaos_events WHERE action = 'inject'",
        [],
        |row| row.get(0),
    )?;

    Ok(())
}

fn ensure_replication_catalog(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS replication_log (
            idx INTEGER PRIMARY KEY,
            term INTEGER NOT NULL,
            leader_id TEXT NOT NULL,
            operation TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            checksum TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL,
            committed INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS election_votes (
            term INTEGER NOT NULL,
            voter_id TEXT NOT NULL,
            candidate_id TEXT NOT NULL,
            granted INTEGER NOT NULL,
            reason TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS replication_reconcile_events (
            node_id TEXT NOT NULL,
            last_applied_index INTEGER NOT NULL,
            commit_index INTEGER NOT NULL,
            replication_lag_ms INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS ha_runtime_markers (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS ha_resilience_events (
            event_type TEXT NOT NULL,
            duration_ms INTEGER,
            note TEXT,
            created_at_unix_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ha_chaos_events (
            action TEXT NOT NULL,
            scenario TEXT NOT NULL,
            created_at_unix_ms INTEGER NOT NULL
        );
        ",
    )?;
    Ok(())
}

fn append_replication_entry_to_store(
    db_path: &Path,
    entry: &ReplicationLogEntry,
    committed: bool,
) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT OR REPLACE INTO replication_log
            (idx, term, leader_id, operation, payload_json, checksum, created_at_unix_ms, committed)
        VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ",
        params![
            entry.index,
            entry.term,
            entry.leader_id,
            entry.operation,
            serde_json::to_string(&entry.payload)?,
            entry.checksum,
            entry.created_at_unix_ms,
            if committed { 1 } else { 0 },
        ],
    )?;
    Ok(())
}

fn rewrite_replication_log_store(
    db_path: &Path,
    log: &ReplicationLog,
    commit_index: u64,
) -> Result<()> {
    let mut conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM replication_log", [])?;

    for entry in log.entries() {
        tx.execute(
            "
            INSERT INTO replication_log
                (idx, term, leader_id, operation, payload_json, checksum, created_at_unix_ms, committed)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                entry.index,
                entry.term,
                entry.leader_id,
                entry.operation,
                serde_json::to_string(&entry.payload)?,
                entry.checksum,
                entry.created_at_unix_ms,
                if entry.index <= commit_index { 1 } else { 0 },
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn mark_committed_replication_entries(db_path: &Path, commit_index: u64) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "UPDATE replication_log SET committed = 1 WHERE idx <= ?1",
        params![commit_index],
    )?;
    Ok(())
}

fn persist_vote_event(
    db_path: &Path,
    term: u64,
    voter_id: &str,
    candidate_id: &str,
    granted: bool,
    reason: &str,
) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT INTO election_votes (term, voter_id, candidate_id, granted, reason)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ",
        params![
            term,
            voter_id,
            candidate_id,
            if granted { 1 } else { 0 },
            reason
        ],
    )?;
    Ok(())
}

fn persist_reconcile_event(
    db_path: &Path,
    node_id: &str,
    last_applied_index: u64,
    commit_index: u64,
    replication_lag_ms: u64,
) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT INTO replication_reconcile_events (node_id, last_applied_index, commit_index, replication_lag_ms)
        VALUES (?1, ?2, ?3, ?4)
        ",
        params![node_id, last_applied_index, commit_index, replication_lag_ms],
    )?;
    Ok(())
}

fn persist_runtime_marker(db_path: &Path, key: &str, value: &str) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT INTO ha_runtime_markers (key, value, updated_at)
        VALUES (?1, ?2, datetime('now'))
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')
        ",
        params![key, value],
    )?;
    Ok(())
}

fn persist_resilience_event(
    db_path: &Path,
    event_type: &str,
    duration_ms: Option<u64>,
    note: &str,
) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT INTO ha_resilience_events (event_type, duration_ms, note, created_at_unix_ms)
        VALUES (?1, ?2, ?3, ?4)
        ",
        params![
            event_type,
            duration_ms.map(|value| value as i64),
            note,
            unix_ms_now() as i64,
        ],
    )?;
    Ok(())
}

fn persist_chaos_event(db_path: &Path, action: &str, scenario: &str) -> Result<()> {
    let conn = Connection::open(db_path)?;
    ensure_replication_catalog(&conn)?;
    conn.execute(
        "
        INSERT INTO ha_chaos_events (action, scenario, created_at_unix_ms)
        VALUES (?1, ?2, ?3)
        ",
        params![action, scenario, unix_ms_now() as i64],
    )?;
    Ok(())
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        507 => "Insufficient Storage",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };

    let header = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::AuditLogger;
    use crate::{ChunkInput, RbacPolicy, RuntimeConfig};
    use serde_json::json;
    use tempfile::{NamedTempFile, tempdir};

    fn make_request(
        method: &str,
        path: &str,
        body: Option<&str>,
        token: Option<&str>,
    ) -> HttpRequest {
        let mut headers = HashMap::new();
        let body_bytes = body.unwrap_or_default().as_bytes().to_vec();
        if !body_bytes.is_empty() {
            headers.insert("content-length".to_string(), body_bytes.len().to_string());
            headers.insert("content-type".to_string(), "application/json".to_string());
        }
        if let Some(value) = token {
            headers.insert("x-sqlrite-control-token".to_string(), value.to_string());
        }

        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            headers,
            body: body_bytes,
        }
    }

    fn replication_primary_config() -> ServerConfig {
        let mut cfg = ServerConfig::default();
        cfg.ha_profile.replication.enabled = true;
        cfg.ha_profile.replication.role = ServerRole::Primary;
        cfg.ha_profile.replication.sync_ack_quorum = 2;
        cfg.ha_profile.replication.node_id = "node-a".to_string();
        cfg
    }

    fn secure_server_config(audit_log_path: PathBuf) -> ServerConfig {
        ServerConfig {
            security: ServerSecurityConfig {
                secure_defaults: true,
                require_auth_context: true,
                policy: Some(RbacPolicy::default()),
                audit_log_path: Some(audit_log_path),
                ..ServerSecurityConfig::default()
            },
            ..ServerConfig::default()
        }
    }

    #[test]
    fn parses_http_request_line() {
        assert_eq!(
            parse_http_request_line("GET /healthz HTTP/1.1\r\n"),
            Some(("GET", "/healthz"))
        );
        assert_eq!(parse_http_request_line(""), None);
    }

    #[test]
    fn builds_health_response_with_ha_payload() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "health endpoint".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;

        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("GET", "/healthz", None, None);
        let (status, content_type, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &ServerConfig::default(),
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert_eq!(content_type, "application/json");
        assert!(body.contains("storage"));
        assert!(body.contains("ha"));
        Ok(())
    }

    #[test]
    fn control_plane_requires_token_when_configured() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("POST", "/control/v1/failover/start", Some("{}"), None);
        let config = ServerConfig {
            control_api_token: Some("secret".to_string()),
            ..ServerConfig::default()
        };

        let (status, _content_type, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 401);
        assert!(body.contains("unauthorized"));
        Ok(())
    }

    #[test]
    fn sql_endpoint_rejects_non_post() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("GET", "/v1/sql", None, None);

        let (status, _content_type, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &ServerConfig::default(),
            &state,
            &request,
        )?;
        assert_eq!(status, 405);
        assert!(body.contains("method not allowed"));
        Ok(())
    }

    #[test]
    fn sql_endpoint_reports_parse_error() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "agent memory".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;

        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("POST", "/v1/sql", Some("{}"), None);

        let (status, _content_type, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &ServerConfig::default(),
            &state,
            &request,
        )?;
        assert_eq!(status, 400);
        assert!(body.contains("statement"));
        Ok(())
    }

    #[test]
    fn sql_endpoint_executes_search_statement() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "search-sql-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "server sql search endpoint".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "demo"}),
            source: Some("docs/search-sql-1.md".to_string()),
        })?;

        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let body = json!({
            "statement": "SELECT chunk_id, doc_id, hybrid_score FROM SEARCH('server sql', vector('1,0'), 3, 0.65, 500, 'balanced', NULL, NULL) ORDER BY hybrid_score DESC, chunk_id ASC;"
        })
        .to_string();
        let request = make_request("POST", "/v1/sql", Some(&body), None);

        let (status, _content_type, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &ServerConfig::default(),
            &state,
            &request,
        )?;
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("\"chunk_id\":\"search-sql-1\""));
        Ok(())
    }

    #[test]
    fn openapi_endpoint_exposes_query_and_grpc_paths() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("GET", "/v1/openapi.json", None, None);

        let (status, _content_type, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &ServerConfig::default(),
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"/v1/query\""));
        assert!(body.contains("\"/v1/query-compact\""));
        assert!(body.contains("\"/grpc/sqlrite.v1.QueryService/Query\""));
        assert!(body.contains("\"/grpc/sqlrite.v1.QueryService/Sql\""));
        Ok(())
    }

    #[test]
    fn openapi_endpoint_hides_sql_paths_when_sql_endpoint_disabled() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let request = make_request("GET", "/v1/openapi.json", None, None);
        let config = ServerConfig {
            enable_sql_endpoint: false,
            ..ServerConfig::default()
        };

        let (status, _content_type, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert!(!body.contains("\"/v1/sql\""));
        assert!(!body.contains("\"/v1/query\""));
        assert!(!body.contains("\"/grpc/sqlrite.v1.QueryService/Sql\""));
        assert!(!body.contains("\"/grpc/sqlrite.v1.QueryService/Query\""));
        assert!(body.contains("\"/v1/openapi.json\""));
        Ok(())
    }

    #[test]
    fn security_endpoint_reports_secure_defaults_and_roles() -> Result<()> {
        let tmp = tempdir()?;
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = secure_server_config(tmp.path().join("audit.jsonl"));
        let request = make_request("GET", "/control/v1/security", None, None);

        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"enabled\":true"));
        assert!(body.contains("\"require_auth_context\":true"));
        assert!(body.contains("tenant_admin"));
        Ok(())
    }

    #[test]
    fn security_audit_export_endpoint_writes_filtered_jsonl() -> Result<()> {
        let tmp = tempdir()?;
        let audit_path = tmp.path().join("audit.jsonl");
        let export_path = tmp.path().join("export.jsonl");
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let mut config = secure_server_config(audit_path.clone());
        config.control_api_token = Some("secret".to_string());

        let logger = JsonlAuditLogger::new(&audit_path, Vec::<String>::new())?;
        logger.log(&AuditEvent {
            unix_ms: 10,
            actor_id: "reader-1".to_string(),
            tenant_id: "acme".to_string(),
            operation: AccessOperation::Query,
            allowed: true,
            detail: json!({"path":"/v1/query"}),
        })?;
        logger.log(&AuditEvent {
            unix_ms: 20,
            actor_id: "admin-1".to_string(),
            tenant_id: "acme".to_string(),
            operation: AccessOperation::SqlAdmin,
            allowed: false,
            detail: json!({"path":"/v1/sql"}),
        })?;

        let request = make_request(
            "POST",
            "/control/v1/security/audit/export",
            Some(&format!(
                "{{\"actor_id\":\"reader-1\",\"output_path\":\"{}\",\"format\":\"jsonl\"}}",
                export_path.display()
            )),
            Some("secret"),
        );
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"matched_events\":1"));
        let export = std::fs::read_to_string(export_path)?;
        assert!(export.contains("reader-1"));
        assert!(!export.contains("admin-1"));
        Ok(())
    }

    #[test]
    fn secure_query_requires_auth_context_headers() -> Result<()> {
        let tmp = tempdir()?;
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = secure_server_config(tmp.path().join("audit.jsonl"));
        let request = make_request(
            "POST",
            "/v1/query",
            Some(r#"{"query_text":"agent","top_k":1}"#),
            None,
        );

        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 403);
        assert!(body.contains("missing auth context"));
        Ok(())
    }

    #[test]
    fn secure_query_enforces_tenant_headers_and_audits() -> Result<()> {
        let tmp = tempdir()?;
        let audit_path = tmp.path().join("audit.jsonl");
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "tenant-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "tenant scoped agent memory".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = secure_server_config(audit_path.clone());

        let mut ok_request = make_request(
            "POST",
            "/v1/query",
            Some(r#"{"query_text":"agent","top_k":1}"#),
            None,
        );
        ok_request
            .headers
            .insert("x-sqlrite-actor-id".to_string(), "reader-1".to_string());
        ok_request
            .headers
            .insert("x-sqlrite-tenant-id".to_string(), "acme".to_string());
        ok_request
            .headers
            .insert("x-sqlrite-roles".to_string(), "reader".to_string());

        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &ok_request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"row_count\":1"));

        let mut denied_request = make_request(
            "POST",
            "/v1/query",
            Some(r#"{"query_text":"agent","top_k":1,"metadata_filters":{"tenant":"beta"}}"#),
            None,
        );
        denied_request
            .headers
            .insert("x-sqlrite-actor-id".to_string(), "reader-1".to_string());
        denied_request
            .headers
            .insert("x-sqlrite-tenant-id".to_string(), "acme".to_string());
        denied_request
            .headers
            .insert("x-sqlrite-roles".to_string(), "reader".to_string());

        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &denied_request,
        )?;
        assert_eq!(status, 403);
        assert!(body.contains("tenant filter mismatch"));

        let audit = std::fs::read_to_string(audit_path)?;
        assert!(audit.contains("\"allowed\":true"));
        assert!(audit.contains("\"allowed\":false"));
        Ok(())
    }

    #[test]
    fn rerank_hook_endpoint_returns_scored_candidates() -> Result<()> {
        let tmp = tempdir()?;
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "r1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "rerank candidate agent memory".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = secure_server_config(tmp.path().join("audit.jsonl"));

        let mut request = make_request(
            "POST",
            "/v1/rerank-hook",
            Some(r#"{"query_text":"agent","candidate_count":5}"#),
            None,
        );
        request
            .headers
            .insert("x-sqlrite-actor-id".to_string(), "reader-1".to_string());
        request
            .headers
            .insert("x-sqlrite-tenant-id".to_string(), "acme".to_string());
        request
            .headers
            .insert("x-sqlrite-roles".to_string(), "reader".to_string());

        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"kind\":\"rerank_hook\""));
        assert!(body.contains("\"vector_score\""));
        assert!(body.contains("\"text_score\""));
        Ok(())
    }

    #[test]
    fn secure_sql_requires_admin_role() -> Result<()> {
        let tmp = tempdir()?;
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "sql-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "sql secure query".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "acme"}),
            source: None,
        })?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = secure_server_config(tmp.path().join("audit.jsonl"));

        let mut reader_request = make_request(
            "POST",
            "/v1/sql",
            Some(r#"{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}"#),
            None,
        );
        reader_request
            .headers
            .insert("x-sqlrite-actor-id".to_string(), "reader-1".to_string());
        reader_request
            .headers
            .insert("x-sqlrite-tenant-id".to_string(), "acme".to_string());
        reader_request
            .headers
            .insert("x-sqlrite-roles".to_string(), "reader".to_string());

        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &reader_request,
        )?;
        assert_eq!(status, 403);
        assert!(body.contains("authorization denied"));

        let mut admin_request = make_request(
            "POST",
            "/v1/sql",
            Some(r#"{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}"#),
            None,
        );
        admin_request
            .headers
            .insert("x-sqlrite-actor-id".to_string(), "admin-1".to_string());
        admin_request
            .headers
            .insert("x-sqlrite-tenant-id".to_string(), "acme".to_string());
        admin_request
            .headers
            .insert("x-sqlrite-roles".to_string(), "admin".to_string());

        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &admin_request,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("sql-1"));
        Ok(())
    }

    #[test]
    fn query_and_grpc_query_endpoints_return_results() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "query-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "agent query endpoint".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "demo"}),
            source: None,
        })?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = ServerConfig::default();

        let query_req = make_request(
            "POST",
            "/v1/query",
            Some(r#"{"query_text":"agent","top_k":1}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &query_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"kind\":\"query\""));
        assert!(body.contains("\"row_count\":1"));

        let compact_req = make_request(
            "POST",
            "/v1/query-compact",
            Some(r#"{"query_text":"agent","top_k":1,"include_payloads":false}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &compact_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"kind\":\"query_compact\""));
        assert!(body.contains("\"chunk_ids\":[\"query-1\"]"));

        let grpc_query_req = make_request(
            "POST",
            "/grpc/sqlrite.v1.QueryService/Query",
            Some(r#"{"query_text":"agent","top_k":1}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &grpc_query_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"kind\":\"query\""));
        Ok(())
    }

    #[test]
    fn query_and_grpc_endpoints_reject_non_post_methods() -> Result<()> {
        let db = SqlRite::open_in_memory_with_config(RuntimeConfig::default())?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = ServerConfig::default();

        let query_get = make_request("GET", "/v1/query", None, None);
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &query_get,
        )?;
        assert_eq!(status, 405);
        assert!(body.contains("POST /v1/query"));

        let compact_get = make_request("GET", "/v1/query-compact", None, None);
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &compact_get,
        )?;
        assert_eq!(status, 405);
        assert!(body.contains("POST /v1/query-compact"));

        let grpc_sql_get = make_request("GET", "/grpc/sqlrite.v1.QueryService/Sql", None, None);
        let (status, _, body) = build_response(
            &db,
            Path::new(":memory:"),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &grpc_sql_get,
        )?;
        assert_eq!(status, 405);
        assert!(body.contains("POST /grpc/sqlrite.v1.QueryService/Sql"));
        Ok(())
    }

    #[test]
    fn grpc_sql_endpoint_executes_sql_statement() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "grpc-sql-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "grpc sql endpoint".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            HaRuntimeProfile::default(),
        )));
        let config = ServerConfig::default();

        let grpc_sql_req = make_request(
            "POST",
            "/grpc/sqlrite.v1.QueryService/Sql",
            Some(r#"{"statement":"SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 1;"}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &grpc_sql_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"kind\":\"query\""));
        assert!(body.contains("\"row_count\":1"));
        assert!(body.contains("grpc-sql-1"));
        Ok(())
    }

    #[test]
    fn replication_append_and_ack_advances_commit_index() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let config = replication_primary_config();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let append = make_request(
            "POST",
            "/control/v1/replication/append",
            Some(r#"{"operation":"ingest_chunk","payload":{"id":"c1"}}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &append,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("commit_index"));

        let ack = make_request(
            "POST",
            "/control/v1/replication/ack",
            Some(r#"{"node_id":"node-b","index":1}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &ack,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"commit_index\":1"));
        Ok(())
    }

    #[test]
    fn election_vote_rejects_stale_term() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let mut config = replication_primary_config();
        config.ha_profile.replication.role = ServerRole::Replica;
        config.ha_profile.replication.node_id = "node-b".to_string();
        let mut state_inner = ControlPlaneState::new(config.ha_profile.clone());
        state_inner.runtime.current_term = 5;
        state_inner.runtime.note_log_position(10, 5);
        let state = Arc::new(Mutex::new(state_inner));

        let vote = make_request(
            "POST",
            "/control/v1/election/request-vote",
            Some(
                r#"{"term":4,"candidate_id":"node-a","candidate_last_log_index":10,"candidate_last_log_term":5}"#,
            ),
            None,
        );

        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &vote,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"vote_granted\":false"));
        assert!(body.contains("stale_term"));
        Ok(())
    }

    #[test]
    fn reconciliation_returns_missing_entries() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let config = replication_primary_config();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        for i in 1..=3 {
            let append = make_request(
                "POST",
                "/control/v1/replication/append",
                Some(&format!(
                    "{{\"operation\":\"write\",\"payload\":{{\"n\":{i}}}}}"
                )),
                None,
            );
            let _ = build_response(
                &db,
                db_file.path(),
                DurabilityProfile::Balanced,
                &config,
                &state,
                &append,
            )?;
        }

        let reconcile = make_request(
            "POST",
            "/control/v1/replication/reconcile",
            Some(r#"{"node_id":"node-c","last_applied_index":1}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &reconcile,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("missing_entries_count"));
        assert!(body.contains("\"index\":2"));
        Ok(())
    }

    #[test]
    fn auto_failover_check_promotes_replica_when_forced() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let mut config = replication_primary_config();
        config.ha_profile.replication.role = ServerRole::Replica;
        config.ha_profile.replication.node_id = "node-b".to_string();
        config.ha_profile.replication.failover_mode = FailoverMode::Automatic;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let check = make_request(
            "POST",
            "/control/v1/failover/auto-check",
            Some(r#"{"force":true,"reason":"test_auto"}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &check,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"triggered\":true"));
        assert!(body.contains("\"role\":\"primary\""));
        Ok(())
    }

    #[test]
    fn auto_failover_check_accepts_missing_force_field() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let mut config = replication_primary_config();
        config.ha_profile.replication.role = ServerRole::Replica;
        config.ha_profile.replication.node_id = "node-b".to_string();
        config.ha_profile.replication.failover_mode = FailoverMode::Automatic;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let check = make_request(
            "POST",
            "/control/v1/failover/auto-check",
            Some(r#"{"simulate_elapsed_ms":5000,"reason":"timeout-path"}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &check,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"triggered\":true"));
        assert!(body.contains("\"reason\":\"timeout-path\""));
        Ok(())
    }

    #[test]
    fn chaos_disk_full_blocks_replication_append() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let config = replication_primary_config();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let inject = make_request(
            "POST",
            "/control/v1/chaos/inject",
            Some(r#"{"scenario":"disk_full"}"#),
            None,
        );
        let (status, _, _) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &inject,
        )?;
        assert_eq!(status, 200);

        let append = make_request(
            "POST",
            "/control/v1/replication/append",
            Some(r#"{"operation":"ingest_chunk","payload":{"id":"c1"}}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &append,
        )?;
        assert_eq!(status, 507);
        assert!(body.contains("disk_full"));
        Ok(())
    }

    #[test]
    fn chaos_node_crash_blocks_health_but_not_status_endpoint() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let config = replication_primary_config();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let inject = make_request(
            "POST",
            "/control/v1/chaos/inject",
            Some(r#"{"scenario":"node_crash"}"#),
            None,
        );
        let _ = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &inject,
        )?;

        let health = make_request("GET", "/readyz", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &health,
        )?;
        assert_eq!(status, 503);
        assert!(body.contains("node_crash"));

        let status_req = make_request("GET", "/control/v1/chaos/status", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &status_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("active_fault_count"));
        Ok(())
    }

    #[test]
    fn recovery_timing_is_recorded_between_start_and_mark_restored() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let config = replication_primary_config();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let start = make_request("POST", "/control/v1/recovery/start", Some(r#"{}"#), None);
        let (status, _, _) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &start,
        )?;
        assert_eq!(status, 200);

        let done = make_request(
            "POST",
            "/control/v1/recovery/mark-restored",
            Some(r#"{"note":"restore_done"}"#),
            None,
        );
        let (status, _, _) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &done,
        )?;
        assert_eq!(status, 200);

        let resilience = make_request("GET", "/control/v1/resilience", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &resilience,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("restore_completed_total"));
        assert!(body.contains("last_restore_duration_ms"));
        Ok(())
    }

    #[test]
    fn recovery_snapshot_verify_and_prune_endpoints_work() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "c1".to_string(),
            doc_id: "d1".to_string(),
            content: "snapshot payload".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        let backup_root = tempdir().map_err(std::io::Error::other)?;
        let mut config = replication_primary_config();
        config.ha_profile.recovery.backup_dir = backup_root.path().display().to_string();
        config.ha_profile.recovery.snapshot_interval_seconds = 1;
        config.ha_profile.recovery.pitr_retention_seconds = 60;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let snapshot_req = make_request(
            "POST",
            "/control/v1/recovery/snapshot",
            Some(r#"{"note":"s17_snapshot"}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &snapshot_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("snapshot_id"));

        let list_req = make_request("GET", "/control/v1/recovery/snapshots?limit=10", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &list_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"count\":1"));

        let verify_req = make_request(
            "POST",
            "/control/v1/recovery/verify-restore",
            Some(r#"{"keep_artifact":false}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &verify_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"restore_verified\":true"));

        let prune_req = make_request(
            "POST",
            "/control/v1/recovery/prune-snapshots",
            Some(r#"{"retention_seconds":0}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &prune_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("removed_count"));
        Ok(())
    }

    #[test]
    fn observability_metrics_and_trace_endpoints_report_sql_activity() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "q1".to_string(),
            doc_id: "d1".to_string(),
            content: "query trace".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({}),
            source: None,
        })?;
        let config = ServerConfig::default();
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        let sql_req = make_request(
            "POST",
            "/v1/sql",
            Some(r#"{"statement":"SELECT id FROM chunks ORDER BY id ASC LIMIT 1;"}"#),
            None,
        );
        let started = unix_ms_now();
        let (status, _, _) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &sql_req,
        )?;
        let duration_ms = unix_ms_now().saturating_sub(started);
        {
            let mut guard = state
                .lock()
                .map_err(|_| std::io::Error::other("state lock poisoned"))?;
            guard
                .observability
                .record_request("POST", "/v1/sql", status, duration_ms);
        }

        let trace_req = make_request("GET", "/control/v1/traces/recent?limit=5", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &trace_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"/v1/sql\""));

        let map_req = make_request("GET", "/control/v1/observability/metrics-map", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &map_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("sqlrite_requests_sql_total"));
        Ok(())
    }

    #[test]
    fn alert_simulation_and_slo_report_reflect_thresholds() -> Result<()> {
        let db_file = NamedTempFile::new().map_err(std::io::Error::other)?;
        let db = SqlRite::open_with_config(db_file.path(), RuntimeConfig::default())?;
        let mut config = replication_primary_config();
        config.ha_profile.replication.max_replication_lag_ms = 50;
        let state = Arc::new(Mutex::new(ControlPlaneState::new(
            config.ha_profile.clone(),
        )));

        {
            let mut guard = state
                .lock()
                .map_err(|_| std::io::Error::other("state lock poisoned"))?;
            guard.runtime.replication_lag_ms = 120;
            guard.observability.record_request("GET", "/readyz", 200, 1);
            guard.observability.record_request("GET", "/readyz", 503, 1);
        }

        let simulate_req = make_request(
            "POST",
            "/control/v1/alerts/simulate",
            Some(r#"{"sql_error_rate":0.20,"sql_avg_latency_ms":75.0,"replication_lag_ms":120}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &simulate_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("replication_lag_high"));
        assert!(body.contains("sql_error_rate_high"));

        let slo_req = make_request("GET", "/control/v1/slo/report", None, None);
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &slo_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("target_percent"));
        assert!(body.contains("target_seconds"));

        let reset_req = make_request(
            "POST",
            "/control/v1/observability/reset",
            Some(r#"{}"#),
            None,
        );
        let (status, _, body) = build_response(
            &db,
            db_file.path(),
            DurabilityProfile::Balanced,
            &config,
            &state,
            &reset_req,
        )?;
        assert_eq!(status, 200);
        assert!(body.contains("\"requests_total\":0"));
        Ok(())
    }
}
