use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServerRole {
    #[default]
    Standalone,
    Primary,
    Replica,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailoverMode {
    #[default]
    Manual,
    Automatic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub enabled: bool,
    pub cluster_id: String,
    pub node_id: String,
    pub role: ServerRole,
    pub advertise_addr: String,
    pub peers: Vec<String>,
    pub sync_ack_quorum: usize,
    pub heartbeat_interval_ms: u64,
    pub election_timeout_ms: u64,
    pub max_replication_lag_ms: u64,
    pub failover_mode: FailoverMode,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cluster_id: "local-cluster".to_string(),
            node_id: "node-1".to_string(),
            role: ServerRole::Standalone,
            advertise_addr: "127.0.0.1:8099".to_string(),
            peers: Vec::new(),
            sync_ack_quorum: 1,
            heartbeat_interval_ms: 1_000,
            election_timeout_ms: 3_000,
            max_replication_lag_ms: 2_000,
            failover_mode: FailoverMode::Manual,
        }
    }
}

impl ReplicationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            if self.role != ServerRole::Standalone {
                return Err(
                    "replication role must be `standalone` when replication is disabled"
                        .to_string(),
                );
            }
            if !self.peers.is_empty() {
                return Err(
                    "replication peers are not allowed when replication is disabled".to_string(),
                );
            }
            return Ok(());
        }

        if self.cluster_id.trim().is_empty() {
            return Err(
                "replication cluster_id cannot be empty when replication is enabled".to_string(),
            );
        }
        if self.node_id.trim().is_empty() {
            return Err(
                "replication node_id cannot be empty when replication is enabled".to_string(),
            );
        }
        if self.advertise_addr.trim().is_empty() {
            return Err(
                "replication advertise_addr cannot be empty when replication is enabled"
                    .to_string(),
            );
        }
        if self.role == ServerRole::Standalone {
            return Err(
                "replication role must be `primary` or `replica` when replication is enabled"
                    .to_string(),
            );
        }
        if self.sync_ack_quorum == 0 {
            return Err("replication sync_ack_quorum must be at least 1".to_string());
        }
        if self.heartbeat_interval_ms == 0 {
            return Err("replication heartbeat_interval_ms must be greater than 0".to_string());
        }
        if self.election_timeout_ms <= self.heartbeat_interval_ms {
            return Err(
                "replication election_timeout_ms must be greater than heartbeat_interval_ms"
                    .to_string(),
            );
        }
        if self.max_replication_lag_ms == 0 {
            return Err("replication max_replication_lag_ms must be greater than 0".to_string());
        }

        if self.role == ServerRole::Primary {
            let cluster_size = self.peers.len() + 1;
            if self.sync_ack_quorum > cluster_size {
                return Err(format!(
                    "replication sync_ack_quorum {} exceeds cluster size {}",
                    self.sync_ack_quorum, cluster_size
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    pub backup_dir: String,
    pub snapshot_interval_seconds: u64,
    pub pitr_retention_seconds: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            backup_dir: "./backups".to_string(),
            snapshot_interval_seconds: 300,
            pitr_retention_seconds: 86_400,
        }
    }
}

impl RecoveryConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.backup_dir.trim().is_empty() {
            return Err("recovery backup_dir cannot be empty".to_string());
        }
        if self.snapshot_interval_seconds == 0 {
            return Err("recovery snapshot_interval_seconds must be greater than 0".to_string());
        }
        if self.pitr_retention_seconds < self.snapshot_interval_seconds {
            return Err(
                "recovery pitr_retention_seconds must be >= snapshot_interval_seconds".to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HaRuntimeProfile {
    pub replication: ReplicationConfig,
    pub recovery: RecoveryConfig,
}

impl HaRuntimeProfile {
    pub fn validate(&self) -> Result<(), String> {
        self.replication.validate()?;
        self.recovery.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HaRuntimeState {
    pub role: ServerRole,
    pub leader_id: Option<String>,
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub commit_index: u64,
    pub last_applied_index: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
    pub replication_lag_ms: u64,
    pub failover_in_progress: bool,
    pub last_transition_unix_ms: u64,
    pub last_heartbeat_unix_ms: Option<u64>,
    pub last_recovery_event: Option<String>,
}

impl HaRuntimeState {
    pub fn new(profile: &HaRuntimeProfile) -> Self {
        let role = profile.replication.role;
        let leader_id = if role == ServerRole::Primary {
            Some(profile.replication.node_id.clone())
        } else {
            None
        };
        Self {
            role,
            leader_id,
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied_index: 0,
            last_log_index: 0,
            last_log_term: 0,
            replication_lag_ms: 0,
            failover_in_progress: false,
            last_transition_unix_ms: unix_ms_now(),
            last_heartbeat_unix_ms: None,
            last_recovery_event: None,
        }
    }

    pub fn promote_to_primary(&mut self, node_id: String) {
        self.role = ServerRole::Primary;
        self.current_term = self.current_term.saturating_add(1).max(1);
        self.voted_for = Some(node_id.clone());
        self.leader_id = Some(node_id);
        self.failover_in_progress = false;
        self.last_transition_unix_ms = unix_ms_now();
    }

    pub fn step_down_to_replica(&mut self, leader_id: Option<String>) {
        self.role = ServerRole::Replica;
        self.leader_id = leader_id;
        self.failover_in_progress = false;
        self.last_transition_unix_ms = unix_ms_now();
    }

    pub fn mark_failover_started(&mut self) {
        self.failover_in_progress = true;
        self.last_transition_unix_ms = unix_ms_now();
    }

    pub fn mark_heartbeat(
        &mut self,
        leader_id: Option<String>,
        commit_index: u64,
        replication_lag_ms: u64,
    ) {
        self.leader_id = leader_id;
        self.commit_index = self.commit_index.max(commit_index).min(self.last_log_index);
        self.last_applied_index = self.last_applied_index.max(self.commit_index);
        self.replication_lag_ms = replication_lag_ms;
        self.last_heartbeat_unix_ms = Some(unix_ms_now());
    }

    pub fn mark_recovery_event(&mut self, event: String) {
        self.last_recovery_event = Some(event);
        self.last_transition_unix_ms = unix_ms_now();
    }

    pub fn adopt_term(&mut self, term: u64) {
        if term > self.current_term {
            self.current_term = term;
            self.voted_for = None;
            if self.role == ServerRole::Primary {
                self.role = ServerRole::Replica;
            }
            self.last_transition_unix_ms = unix_ms_now();
        }
    }

    pub fn grant_vote(&mut self, term: u64, candidate_id: String) {
        self.adopt_term(term);
        self.voted_for = Some(candidate_id);
        self.last_transition_unix_ms = unix_ms_now();
    }

    pub fn can_grant_vote(
        &self,
        term: u64,
        candidate_id: &str,
        candidate_last_log_index: u64,
        candidate_last_log_term: u64,
    ) -> bool {
        if term < self.current_term {
            return false;
        }
        if term == self.current_term
            && self
                .voted_for
                .as_ref()
                .is_some_and(|voted| voted != candidate_id)
        {
            return false;
        }

        let candidate_up_to_date = candidate_last_log_term > self.last_log_term
            || (candidate_last_log_term == self.last_log_term
                && candidate_last_log_index >= self.last_log_index);
        candidate_up_to_date
    }

    pub fn note_log_position(&mut self, last_log_index: u64, last_log_term: u64) {
        self.last_log_index = last_log_index;
        self.last_log_term = last_log_term;
    }

    pub fn advance_commit_index(&mut self, commit_index: u64) {
        self.commit_index = commit_index.min(self.last_log_index).max(self.commit_index);
        self.last_applied_index = self.last_applied_index.max(self.commit_index);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplicationLogEntry {
    pub index: u64,
    pub term: u64,
    pub leader_id: String,
    pub operation: String,
    pub payload: Value,
    pub checksum: String,
    pub created_at_unix_ms: u64,
}

impl ReplicationLogEntry {
    pub fn new(
        index: u64,
        term: u64,
        leader_id: String,
        operation: String,
        payload: Value,
    ) -> Result<Self, String> {
        if index == 0 {
            return Err("replication log index must be >= 1".to_string());
        }
        if term == 0 {
            return Err("replication log term must be >= 1".to_string());
        }
        if leader_id.trim().is_empty() {
            return Err("replication log leader_id cannot be empty".to_string());
        }
        if operation.trim().is_empty() {
            return Err("replication log operation cannot be empty".to_string());
        }
        let checksum = compute_log_checksum(index, term, &leader_id, &operation, &payload)?;
        Ok(Self {
            index,
            term,
            leader_id,
            operation,
            payload,
            checksum,
            created_at_unix_ms: unix_ms_now(),
        })
    }

    pub fn verify_checksum(&self) -> bool {
        compute_log_checksum(
            self.index,
            self.term,
            &self.leader_id,
            &self.operation,
            &self.payload,
        )
        .is_ok_and(|checksum| checksum == self.checksum)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReplicationLog {
    entries: Vec<ReplicationLogEntry>,
    acked_by: HashMap<u64, HashSet<String>>,
}

impl ReplicationLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_entries(entries: Vec<ReplicationLogEntry>) -> Result<Self, String> {
        let mut out = Self::new();
        if entries.is_empty() {
            return Ok(out);
        }

        let mut sorted = entries;
        sorted.sort_by_key(|entry| entry.index);
        for (position, entry) in sorted.iter().enumerate() {
            let expected_index = (position as u64) + 1;
            if entry.index != expected_index {
                return Err(format!(
                    "replication log index gap: expected {}, found {}",
                    expected_index, entry.index
                ));
            }
            if !entry.verify_checksum() {
                return Err(format!(
                    "replication log checksum mismatch at index {}",
                    entry.index
                ));
            }
        }

        out.entries = sorted;
        Ok(out)
    }

    pub fn entries(&self) -> &[ReplicationLogEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn last_index(&self) -> u64 {
        self.entries.last().map(|entry| entry.index).unwrap_or(0)
    }

    pub fn last_term(&self) -> u64 {
        self.entries.last().map(|entry| entry.term).unwrap_or(0)
    }

    pub fn entry_at(&self, index: u64) -> Option<&ReplicationLogEntry> {
        if index == 0 {
            return None;
        }
        self.entries.get((index - 1) as usize)
    }

    pub fn entries_from(&self, start_index: u64, limit: usize) -> Vec<ReplicationLogEntry> {
        if limit == 0 {
            return Vec::new();
        }
        self.entries
            .iter()
            .filter(|entry| entry.index >= start_index)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn append_leader_event(
        &mut self,
        term: u64,
        leader_id: &str,
        operation: String,
        payload: Value,
        local_node_id: &str,
    ) -> Result<ReplicationLogEntry, String> {
        let index = self.last_index().saturating_add(1);
        let entry =
            ReplicationLogEntry::new(index, term, leader_id.to_string(), operation, payload)?;
        self.entries.push(entry.clone());
        self.acknowledge(index, local_node_id.to_string());
        Ok(entry)
    }

    pub fn append_remote_entries(
        &mut self,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: &[ReplicationLogEntry],
    ) -> Result<(), String> {
        if prev_log_index > self.last_index() {
            return Err(format!(
                "replication log mismatch: prev_log_index {} beyond local last_index {}",
                prev_log_index,
                self.last_index()
            ));
        }
        if prev_log_index > 0 {
            let local_prev = self
                .entry_at(prev_log_index)
                .ok_or_else(|| "replication log previous entry missing".to_string())?;
            if local_prev.term != prev_log_term {
                return Err(format!(
                    "replication log mismatch: prev term {} does not match local term {} at index {}",
                    prev_log_term, local_prev.term, prev_log_index
                ));
            }
        }

        let mut cursor = prev_log_index;
        for incoming in entries {
            cursor = cursor.saturating_add(1);
            if incoming.index != cursor {
                return Err(format!(
                    "replication log sequence mismatch: expected incoming index {}, found {}",
                    cursor, incoming.index
                ));
            }
            if !incoming.verify_checksum() {
                return Err(format!(
                    "replication log checksum mismatch at incoming index {}",
                    incoming.index
                ));
            }

            if let Some(existing) = self.entry_at(incoming.index) {
                if existing.term != incoming.term || existing.checksum != incoming.checksum {
                    self.truncate_from(incoming.index);
                }
            }
            if self.entry_at(incoming.index).is_none() {
                self.entries.push(incoming.clone());
            }
        }

        Ok(())
    }

    pub fn acknowledge(&mut self, index: u64, node_id: String) -> usize {
        if index == 0 || index > self.last_index() {
            return 0;
        }
        let node_set = self.acked_by.entry(index).or_default();
        node_set.insert(node_id);
        node_set.len()
    }

    pub fn ack_count(&self, index: u64) -> usize {
        self.acked_by
            .get(&index)
            .map(HashSet::len)
            .unwrap_or_default()
    }

    pub fn compute_commit_index(&self, current_commit_index: u64, quorum: usize) -> u64 {
        if quorum == 0 {
            return current_commit_index;
        }
        let mut next = current_commit_index.saturating_add(1);
        let mut committed = current_commit_index;
        while next <= self.last_index() {
            if self.ack_count(next) >= quorum {
                committed = next;
                next = next.saturating_add(1);
            } else {
                break;
            }
        }
        committed
    }

    fn truncate_from(&mut self, index: u64) {
        if index == 0 {
            self.entries.clear();
            self.acked_by.clear();
            return;
        }
        self.entries.retain(|entry| entry.index < index);
        self.acked_by.retain(|entry_index, _| *entry_index < index);
    }
}

fn compute_log_checksum(
    index: u64,
    term: u64,
    leader_id: &str,
    operation: &str,
    payload: &Value,
) -> Result<String, String> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|error| format!("checksum payload error: {error}"))?;
    let raw = format!("{index}|{term}|{leader_id}|{operation}|{payload_json}");
    Ok(format!("{:016x}", fnv1a64(raw.as_bytes())))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn standalone_profile_is_valid() {
        let profile = HaRuntimeProfile::default();
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn enabled_replication_requires_ha_role() {
        let mut profile = HaRuntimeProfile::default();
        profile.replication.enabled = true;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn primary_quorum_cannot_exceed_cluster_size() {
        let mut profile = HaRuntimeProfile::default();
        profile.replication.enabled = true;
        profile.replication.role = ServerRole::Primary;
        profile.replication.peers = vec!["n2".to_string()];
        profile.replication.sync_ack_quorum = 3;
        assert!(profile.validate().is_err());
    }

    #[test]
    fn state_transitions_record_role_changes() {
        let mut profile = HaRuntimeProfile::default();
        profile.replication.enabled = true;
        profile.replication.role = ServerRole::Replica;
        let mut state = HaRuntimeState::new(&profile);
        assert_eq!(state.role, ServerRole::Replica);
        state.mark_failover_started();
        assert!(state.failover_in_progress);
        state.promote_to_primary("node-a".to_string());
        assert_eq!(state.role, ServerRole::Primary);
        assert_eq!(state.leader_id.as_deref(), Some("node-a"));
        assert!(!state.failover_in_progress);
    }

    #[test]
    fn vote_guard_rejects_stale_candidate_logs() {
        let mut profile = HaRuntimeProfile::default();
        profile.replication.enabled = true;
        profile.replication.role = ServerRole::Replica;
        let mut state = HaRuntimeState::new(&profile);
        state.current_term = 5;
        state.note_log_position(10, 5);
        assert!(!state.can_grant_vote(5, "node-b", 9, 5));
        assert!(state.can_grant_vote(5, "node-b", 10, 5));
    }

    #[test]
    fn replication_log_appends_and_commits_with_quorum() {
        let mut log = ReplicationLog::new();
        let entry = log
            .append_leader_event(
                1,
                "node-a",
                "ingest_chunk".to_string(),
                json!({"id": "c1"}),
                "node-a",
            )
            .expect("append must succeed");
        assert_eq!(entry.index, 1);
        assert_eq!(log.ack_count(1), 1);
        log.acknowledge(1, "node-b".to_string());
        assert_eq!(log.compute_commit_index(0, 2), 1);
    }

    #[test]
    fn replication_log_conflict_truncates_suffix() {
        let mut log = ReplicationLog::new();
        let _ = log
            .append_leader_event(1, "node-a", "write".to_string(), json!({"k": 1}), "node-a")
            .expect("append 1");
        let _ = log
            .append_leader_event(1, "node-a", "write".to_string(), json!({"k": 2}), "node-a")
            .expect("append 2");

        let replacement = vec![
            ReplicationLogEntry::new(
                2,
                2,
                "node-b".to_string(),
                "write".to_string(),
                json!({"k": 20}),
            )
            .expect("entry"),
            ReplicationLogEntry::new(
                3,
                2,
                "node-b".to_string(),
                "write".to_string(),
                json!({"k": 30}),
            )
            .expect("entry"),
        ];
        log.append_remote_entries(1, 1, &replacement)
            .expect("append replacement");

        assert_eq!(log.last_index(), 3);
        assert_eq!(log.entry_at(2).map(|entry| entry.term), Some(2));
        assert_eq!(log.entry_at(3).map(|entry| entry.term), Some(2));
    }

    #[test]
    fn replication_log_rejects_bad_checksum() {
        let mut entry = ReplicationLogEntry::new(
            1,
            1,
            "node-a".to_string(),
            "write".to_string(),
            json!({"x": 1}),
        )
        .expect("entry");
        entry.checksum = "deadbeef".to_string();

        let mut log = ReplicationLog::new();
        let err = log
            .append_remote_entries(0, 0, &[entry])
            .expect_err("checksum must fail");
        assert!(err.contains("checksum"));
    }
}
