use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem::size_of;

use crate::{Result, SqlRiteError};
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIndexMode {
    Disabled,
    BruteForce,
    LshAnn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorCandidate {
    pub chunk_id: String,
    pub score: f32,
}

pub trait VectorIndex {
    fn name(&self) -> &'static str;
    fn dimension(&self) -> Option<usize>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn estimated_memory_bytes(&self) -> usize;
    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()>;
    fn upsert_batch(&mut self, items: &[(&str, &[f32])]) -> Result<()> {
        for (chunk_id, embedding) in items {
            self.upsert(chunk_id, embedding)?;
        }
        Ok(())
    }
    fn remove(&mut self, chunk_id: &str) -> Result<()>;
    fn reset(&mut self) -> Result<()>;
    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>>;
}

#[derive(Debug, Clone)]
pub(crate) enum BuiltinVectorIndex {
    BruteForce(BruteForceVectorIndex),
    LshAnn(LshAnnVectorIndex),
}

impl BuiltinVectorIndex {
    pub(crate) fn from_mode(mode: VectorIndexMode) -> Option<Self> {
        match mode {
            VectorIndexMode::Disabled => None,
            VectorIndexMode::BruteForce => Some(Self::BruteForce(BruteForceVectorIndex::new())),
            VectorIndexMode::LshAnn => Some(Self::LshAnn(LshAnnVectorIndex::new())),
        }
    }
}

impl VectorIndex for BuiltinVectorIndex {
    fn name(&self) -> &'static str {
        match self {
            Self::BruteForce(index) => index.name(),
            Self::LshAnn(index) => index.name(),
        }
    }

    fn dimension(&self) -> Option<usize> {
        match self {
            Self::BruteForce(index) => index.dimension(),
            Self::LshAnn(index) => index.dimension(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::BruteForce(index) => index.len(),
            Self::LshAnn(index) => index.len(),
        }
    }

    fn estimated_memory_bytes(&self) -> usize {
        match self {
            Self::BruteForce(index) => index.estimated_memory_bytes(),
            Self::LshAnn(index) => index.estimated_memory_bytes(),
        }
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.upsert(chunk_id, embedding),
            Self::LshAnn(index) => index.upsert(chunk_id, embedding),
        }
    }

    fn upsert_batch(&mut self, items: &[(&str, &[f32])]) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.upsert_batch(items),
            Self::LshAnn(index) => index.upsert_batch(items),
        }
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.remove(chunk_id),
            Self::LshAnn(index) => index.remove(chunk_id),
        }
    }

    fn reset(&mut self) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.reset(),
            Self::LshAnn(index) => index.reset(),
        }
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        match self {
            Self::BruteForce(index) => index.query(query_embedding, limit),
            Self::LshAnn(index) => index.query(query_embedding, limit),
        }
    }
}

#[derive(Debug, Clone)]
struct VectorEntry {
    chunk_id: String,
    normalized_embedding: Vec<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct BruteForceVectorIndex {
    dimension: Option<usize>,
    entries: Vec<VectorEntry>,
    positions: HashMap<String, usize>,
}

const PARALLEL_SCAN_THRESHOLD: usize = 4_096;
const LSH_DEFAULT_BITS_PER_TABLE: usize = 14;
const LSH_DEFAULT_TABLE_COUNT: usize = 6;
const LSH_DEFAULT_MIN_CANDIDATES: usize = 192;
const LSH_DEFAULT_MAX_HAMMING_RADIUS: usize = 2;
const LSH_DEFAULT_MAX_CANDIDATE_MULTIPLIER: usize = 8;
const LSH_PARALLEL_SCORE_THRESHOLD: usize = 2_048;
const BATCH_PARALLEL_PREP_THRESHOLD: usize = 512;

impl BruteForceVectorIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_dimension(&self, embedding: &[f32]) -> Result<()> {
        validate_dimension(self.dimension, embedding)
    }
}

impl VectorIndex for BruteForceVectorIndex {
    fn name(&self) -> &'static str {
        "brute_force"
    }

    fn dimension(&self) -> Option<usize> {
        self.dimension
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn estimated_memory_bytes(&self) -> usize {
        let embedding_bytes = self
            .entries
            .iter()
            .map(|entry| entry.normalized_embedding.len() * size_of::<f32>())
            .sum::<usize>();
        let id_bytes = self
            .entries
            .iter()
            .map(|entry| entry.chunk_id.len())
            .sum::<usize>();
        let positions_overhead =
            self.positions.len() * (size_of::<usize>() + size_of::<String>() + size_of::<usize>());
        embedding_bytes + id_bytes + positions_overhead
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        if self.dimension.is_none() {
            self.dimension = Some(embedding.len());
        }

        let normalized_embedding = normalize_embedding(embedding);
        if let Some(position) = self.positions.get(chunk_id).copied() {
            self.entries[position].normalized_embedding = normalized_embedding;
            return Ok(());
        }

        let position = self.entries.len();
        self.entries.push(VectorEntry {
            chunk_id: chunk_id.to_string(),
            normalized_embedding,
        });
        self.positions.insert(chunk_id.to_string(), position);
        Ok(())
    }

    fn upsert_batch(&mut self, items: &[(&str, &[f32])]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        for (_, embedding) in items {
            self.validate_dimension(embedding)?;
        }
        if self.dimension.is_none() {
            self.dimension = Some(items[0].1.len());
        }

        let prepared: Vec<(String, Vec<f32>)> = if items.len() >= BATCH_PARALLEL_PREP_THRESHOLD {
            items
                .par_iter()
                .map(|(chunk_id, embedding)| {
                    ((*chunk_id).to_string(), normalize_embedding(embedding))
                })
                .collect()
        } else {
            items
                .iter()
                .map(|(chunk_id, embedding)| {
                    ((*chunk_id).to_string(), normalize_embedding(embedding))
                })
                .collect()
        };

        self.entries.reserve(prepared.len());
        self.positions.reserve(prepared.len());
        for (chunk_id, normalized_embedding) in prepared {
            if let Some(position) = self.positions.get(&chunk_id).copied() {
                self.entries[position].normalized_embedding = normalized_embedding;
            } else {
                let position = self.entries.len();
                self.entries.push(VectorEntry {
                    chunk_id: chunk_id.clone(),
                    normalized_embedding,
                });
                self.positions.insert(chunk_id, position);
            }
        }

        Ok(())
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        let Some(position) = self.positions.remove(chunk_id) else {
            return Ok(());
        };

        self.entries.swap_remove(position);
        if position < self.entries.len() {
            let moved_id = self.entries[position].chunk_id.clone();
            self.positions.insert(moved_id, position);
        }

        if self.entries.is_empty() {
            self.dimension = None;
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.entries.clear();
        self.positions.clear();
        self.dimension = None;
        Ok(())
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.entries.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;

        let query_normalized = normalize_embedding(query_embedding);
        let mut results: Vec<VectorCandidate> = if self.entries.len() >= PARALLEL_SCAN_THRESHOLD {
            self.entries
                .par_iter()
                .map(|entry| VectorCandidate {
                    chunk_id: entry.chunk_id.clone(),
                    score: dot_product(&query_normalized, &entry.normalized_embedding),
                })
                .collect()
        } else {
            self.entries
                .iter()
                .map(|entry| VectorCandidate {
                    chunk_id: entry.chunk_id.clone(),
                    score: dot_product(&query_normalized, &entry.normalized_embedding),
                })
                .collect()
        };

        if results.len() > limit {
            let nth = limit - 1;
            results.select_nth_unstable_by(nth, compare_candidates_desc);
            results.truncate(limit);
        }
        results.sort_by(compare_candidates_desc);
        Ok(results)
    }
}

#[derive(Debug, Clone)]
struct LshTable {
    hyperplanes: Vec<Vec<f32>>,
    buckets: HashMap<u64, Vec<usize>>,
}

#[derive(Debug, Clone)]
struct LshEntry {
    chunk_id: String,
    normalized_embedding: Vec<f32>,
    table_keys: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct LshAnnVectorIndex {
    dimension: Option<usize>,
    entries: Vec<LshEntry>,
    positions: HashMap<String, usize>,
    tables: Vec<LshTable>,
    bits_per_table: usize,
    table_count: usize,
    min_candidates: usize,
    max_hamming_radius: usize,
    max_candidate_multiplier: usize,
}

impl Default for LshAnnVectorIndex {
    fn default() -> Self {
        Self {
            dimension: None,
            entries: Vec::new(),
            positions: HashMap::new(),
            tables: Vec::new(),
            bits_per_table: LSH_DEFAULT_BITS_PER_TABLE,
            table_count: LSH_DEFAULT_TABLE_COUNT,
            min_candidates: LSH_DEFAULT_MIN_CANDIDATES,
            max_hamming_radius: LSH_DEFAULT_MAX_HAMMING_RADIUS,
            max_candidate_multiplier: LSH_DEFAULT_MAX_CANDIDATE_MULTIPLIER,
        }
    }
}

impl LshAnnVectorIndex {
    pub fn new() -> Self {
        Self::default()
    }

    fn validate_dimension(&self, embedding: &[f32]) -> Result<()> {
        validate_dimension(self.dimension, embedding)
    }

    fn initialize_tables_if_needed(&mut self, dim: usize) {
        if !self.tables.is_empty() {
            return;
        }

        self.tables.reserve(self.table_count);
        for table_idx in 0..self.table_count {
            let mut hyperplanes = Vec::with_capacity(self.bits_per_table);
            for bit in 0..self.bits_per_table {
                hyperplanes.push(generate_hyperplane(dim, table_idx, bit));
            }
            self.tables.push(LshTable {
                hyperplanes,
                buckets: HashMap::new(),
            });
        }
    }

    fn bucket_key(hyperplanes: &[Vec<f32>], normalized_embedding: &[f32]) -> u64 {
        let mut key = 0u64;
        for (idx, plane) in hyperplanes.iter().enumerate() {
            if dot_product(normalized_embedding, plane) >= 0.0 {
                key |= 1u64 << idx;
            }
        }
        key
    }

    fn bucket_keys_for_embedding(&self, normalized_embedding: &[f32]) -> Vec<u64> {
        self.tables
            .iter()
            .map(|table| Self::bucket_key(&table.hyperplanes, normalized_embedding))
            .collect()
    }

    fn insert_position_into_tables(&mut self, position: usize, table_keys: &[u64]) {
        for (table_idx, key) in table_keys.iter().enumerate() {
            if let Some(table) = self.tables.get_mut(table_idx) {
                table.buckets.entry(*key).or_default().push(position);
            }
        }
    }

    fn remove_position_from_tables(&mut self, position: usize, table_keys: &[u64]) {
        for (table_idx, key) in table_keys.iter().enumerate() {
            let Some(table) = self.tables.get_mut(table_idx) else {
                continue;
            };
            if let Some(bucket) = table.buckets.get_mut(key) {
                bucket.retain(|entry_pos| *entry_pos != position);
                if bucket.is_empty() {
                    table.buckets.remove(key);
                }
            }
        }
    }

    fn rebind_position_in_tables(
        &mut self,
        old_position: usize,
        new_position: usize,
        table_keys: &[u64],
    ) {
        for (table_idx, key) in table_keys.iter().enumerate() {
            let Some(table) = self.tables.get_mut(table_idx) else {
                continue;
            };
            if let Some(bucket) = table.buckets.get_mut(key) {
                for entry_pos in bucket {
                    if *entry_pos == old_position {
                        *entry_pos = new_position;
                    }
                }
            }
        }
    }

    fn insert_bucket_with_limit(
        candidates: &mut HashSet<usize>,
        bucket: &[usize],
        max_candidates: usize,
    ) -> bool {
        if candidates.len() >= max_candidates {
            return true;
        }

        for position in bucket {
            candidates.insert(*position);
            if candidates.len() >= max_candidates {
                return true;
            }
        }

        false
    }

    fn hamming_masks(&self) -> Vec<u64> {
        let mut masks = Vec::new();
        if self.max_hamming_radius >= 1 {
            for bit in 0..self.bits_per_table {
                masks.push(1u64 << bit);
            }
        }
        if self.max_hamming_radius >= 2 {
            for bit_a in 0..self.bits_per_table {
                for bit_b in (bit_a + 1)..self.bits_per_table {
                    masks.push((1u64 << bit_a) | (1u64 << bit_b));
                }
            }
        }
        masks
    }

    fn score_candidates(
        &self,
        normalized_query: &[f32],
        candidate_positions: Vec<usize>,
    ) -> Vec<VectorCandidate> {
        if candidate_positions.len() >= LSH_PARALLEL_SCORE_THRESHOLD {
            candidate_positions
                .par_iter()
                .map(|position| {
                    let entry = &self.entries[*position];
                    VectorCandidate {
                        chunk_id: entry.chunk_id.clone(),
                        score: dot_product(normalized_query, &entry.normalized_embedding),
                    }
                })
                .collect()
        } else {
            candidate_positions
                .into_iter()
                .map(|position| {
                    let entry = &self.entries[position];
                    VectorCandidate {
                        chunk_id: entry.chunk_id.clone(),
                        score: dot_product(normalized_query, &entry.normalized_embedding),
                    }
                })
                .collect()
        }
    }
}

impl VectorIndex for LshAnnVectorIndex {
    fn name(&self) -> &'static str {
        "lsh_ann"
    }

    fn dimension(&self) -> Option<usize> {
        self.dimension
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn estimated_memory_bytes(&self) -> usize {
        let entry_bytes = self
            .entries
            .iter()
            .map(|entry| {
                entry.chunk_id.len()
                    + entry.normalized_embedding.len() * size_of::<f32>()
                    + entry.table_keys.len() * size_of::<u64>()
            })
            .sum::<usize>();
        let positions_overhead =
            self.positions.len() * (size_of::<usize>() + size_of::<String>() + size_of::<usize>());

        let hyperplane_bytes = self
            .tables
            .iter()
            .map(|table| {
                table
                    .hyperplanes
                    .iter()
                    .map(|plane| plane.len() * size_of::<f32>())
                    .sum::<usize>()
            })
            .sum::<usize>();

        let bucket_bytes = self
            .tables
            .iter()
            .map(|table| {
                table
                    .buckets
                    .values()
                    .map(|ids| size_of::<u64>() + ids.len() * size_of::<usize>())
                    .sum::<usize>()
            })
            .sum::<usize>();

        entry_bytes + positions_overhead + hyperplane_bytes + bucket_bytes
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        if self.dimension.is_none() {
            self.dimension = Some(embedding.len());
            self.initialize_tables_if_needed(embedding.len());
        }

        let normalized_embedding = normalize_embedding(embedding);
        let table_keys = self.bucket_keys_for_embedding(&normalized_embedding);

        if let Some(position) = self.positions.get(chunk_id).copied() {
            let old_keys = std::mem::replace(&mut self.entries[position].table_keys, table_keys);
            self.remove_position_from_tables(position, &old_keys);
            self.entries[position].normalized_embedding = normalized_embedding;
            let new_keys = self.entries[position].table_keys.clone();
            self.insert_position_into_tables(position, &new_keys);
            return Ok(());
        }

        let position = self.entries.len();
        self.entries.push(LshEntry {
            chunk_id: chunk_id.to_string(),
            normalized_embedding,
            table_keys: table_keys.clone(),
        });
        self.positions.insert(chunk_id.to_string(), position);
        self.insert_position_into_tables(position, &table_keys);
        Ok(())
    }

    fn upsert_batch(&mut self, items: &[(&str, &[f32])]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        for (_, embedding) in items {
            self.validate_dimension(embedding)?;
        }
        if self.dimension.is_none() {
            self.dimension = Some(items[0].1.len());
            self.initialize_tables_if_needed(items[0].1.len());
        }

        let tables = &self.tables;
        let prepared: Vec<(String, Vec<f32>, Vec<u64>)> = if items.len()
            >= BATCH_PARALLEL_PREP_THRESHOLD
        {
            items
                .par_iter()
                .map(|(chunk_id, embedding)| {
                    let normalized_embedding = normalize_embedding(embedding);
                    let table_keys = tables
                        .iter()
                        .map(|table| Self::bucket_key(&table.hyperplanes, &normalized_embedding))
                        .collect::<Vec<_>>();
                    ((*chunk_id).to_string(), normalized_embedding, table_keys)
                })
                .collect()
        } else {
            items
                .iter()
                .map(|(chunk_id, embedding)| {
                    let normalized_embedding = normalize_embedding(embedding);
                    let table_keys = tables
                        .iter()
                        .map(|table| Self::bucket_key(&table.hyperplanes, &normalized_embedding))
                        .collect::<Vec<_>>();
                    ((*chunk_id).to_string(), normalized_embedding, table_keys)
                })
                .collect()
        };

        self.entries.reserve(prepared.len());
        self.positions.reserve(prepared.len());
        for (chunk_id, normalized_embedding, table_keys) in prepared {
            if let Some(position) = self.positions.get(&chunk_id).copied() {
                let old_keys =
                    std::mem::replace(&mut self.entries[position].table_keys, table_keys);
                self.remove_position_from_tables(position, &old_keys);
                self.entries[position].normalized_embedding = normalized_embedding;
                let new_keys = self.entries[position].table_keys.clone();
                self.insert_position_into_tables(position, &new_keys);
            } else {
                let position = self.entries.len();
                self.entries.push(LshEntry {
                    chunk_id: chunk_id.clone(),
                    normalized_embedding,
                    table_keys: table_keys.clone(),
                });
                self.positions.insert(chunk_id, position);
                self.insert_position_into_tables(position, &table_keys);
            }
        }

        Ok(())
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        let Some(position) = self.positions.remove(chunk_id) else {
            return Ok(());
        };

        let removed = self.entries.swap_remove(position);
        self.remove_position_from_tables(position, &removed.table_keys);

        if position < self.entries.len() {
            let old_position = self.entries.len();
            let moved_id = self.entries[position].chunk_id.clone();
            let moved_keys = self.entries[position].table_keys.clone();
            self.rebind_position_in_tables(old_position, position, &moved_keys);
            self.positions.insert(moved_id, position);
        }

        if self.entries.is_empty() {
            self.dimension = None;
            self.positions.clear();
            self.tables.clear();
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.dimension = None;
        self.entries.clear();
        self.positions.clear();
        self.tables.clear();
        Ok(())
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.entries.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;

        let normalized_query = normalize_embedding(query_embedding);
        let target_candidates = limit.max(self.min_candidates);
        let max_candidates = target_candidates
            .saturating_mul(self.max_candidate_multiplier)
            .max(target_candidates);
        let mut candidates = HashSet::with_capacity(max_candidates);
        let mut query_keys = Vec::with_capacity(self.tables.len());

        for table in &self.tables {
            let key = Self::bucket_key(&table.hyperplanes, &normalized_query);
            query_keys.push(key);
            if let Some(bucket) = table.buckets.get(&key)
                && Self::insert_bucket_with_limit(&mut candidates, bucket, max_candidates)
            {
                break;
            }
        }

        if candidates.len() < target_candidates {
            let masks = self.hamming_masks();
            for (table_idx, table) in self.tables.iter().enumerate() {
                let key = query_keys
                    .get(table_idx)
                    .copied()
                    .unwrap_or_else(|| Self::bucket_key(&table.hyperplanes, &normalized_query));

                for mask in &masks {
                    let neighbor = key ^ mask;
                    if let Some(bucket) = table.buckets.get(&neighbor)
                        && Self::insert_bucket_with_limit(&mut candidates, bucket, max_candidates)
                    {
                        break;
                    }
                    if candidates.len() >= target_candidates {
                        break;
                    }
                }
                if candidates.len() >= target_candidates {
                    break;
                }
            }
        }

        if candidates.len() < limit {
            candidates.extend(0..self.entries.len());
        }

        let candidate_positions: Vec<usize> = candidates.into_iter().collect();
        let mut results = self.score_candidates(&normalized_query, candidate_positions);

        if results.len() > limit {
            let nth = limit - 1;
            results.select_nth_unstable_by(nth, compare_candidates_desc);
            results.truncate(limit);
        }
        results.sort_by(compare_candidates_desc);
        Ok(results)
    }
}

fn validate_dimension(expected_dimension: Option<usize>, embedding: &[f32]) -> Result<()> {
    if embedding.is_empty() {
        return Err(SqlRiteError::EmptyEmbedding);
    }

    if let Some(expected) = expected_dimension
        && expected != embedding.len()
    {
        return Err(SqlRiteError::EmbeddingDimensionMismatch {
            expected,
            found: embedding.len(),
        });
    }

    Ok(())
}

fn compare_candidates_desc(left: &VectorCandidate, right: &VectorCandidate) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.chunk_id.cmp(&right.chunk_id))
}

fn normalize_embedding(embedding: &[f32]) -> Vec<f32> {
    let norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return embedding.to_vec();
    }
    embedding.iter().map(|value| value / norm).collect()
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right.iter()).map(|(a, b)| a * b).sum()
}

fn generate_hyperplane(dim: usize, table_idx: usize, bit_idx: usize) -> Vec<f32> {
    let mut plane = Vec::with_capacity(dim);
    for idx in 0..dim {
        let seed = ((table_idx as u64 + 1) << 42) ^ ((bit_idx as u64 + 1) << 21) ^ (idx as u64 + 1);
        plane.push(pseudo_uniform(seed) * 2.0 - 1.0);
    }
    normalize_embedding(&plane)
}

fn pseudo_uniform(seed: u64) -> f32 {
    let mut x = seed.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^= x >> 31;
    (x as f64 / u64::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_index_contract<I: VectorIndex>(mut index: I) -> Result<()> {
        index.upsert("c1", &[1.0, 0.0, 0.0])?;
        index.upsert("c2", &[0.0, 1.0, 0.0])?;
        index.upsert("c3", &[0.8, 0.2, 0.0])?;

        let found = index.query(&[0.9, 0.1, 0.0], 2)?;
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].chunk_id, "c1");
        Ok(())
    }

    #[test]
    fn brute_force_queries_by_similarity() -> Result<()> {
        run_index_contract(BruteForceVectorIndex::new())
    }

    #[test]
    fn lsh_ann_queries_by_similarity() -> Result<()> {
        run_index_contract(LshAnnVectorIndex::new())
    }

    #[test]
    fn brute_force_rejects_dimension_mismatch() -> Result<()> {
        let mut index = BruteForceVectorIndex::new();
        index.upsert("c1", &[1.0, 0.0, 0.0])?;
        let err = index
            .upsert("c2", &[1.0, 0.0])
            .expect_err("mismatch should fail");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn lsh_ann_rejects_dimension_mismatch() -> Result<()> {
        let mut index = LshAnnVectorIndex::new();
        index.upsert("c1", &[1.0, 0.0, 0.0])?;
        let err = index
            .upsert("c2", &[1.0, 0.0])
            .expect_err("mismatch should fail");
        assert!(matches!(
            err,
            SqlRiteError::EmbeddingDimensionMismatch { .. }
        ));
        Ok(())
    }

    #[test]
    fn brute_force_remove_and_reinsert_is_consistent() -> Result<()> {
        let mut index = BruteForceVectorIndex::new();
        index.upsert("c1", &[1.0, 0.0])?;
        index.upsert("c2", &[0.0, 1.0])?;
        index.remove("c1")?;
        index.upsert("c3", &[1.0, 0.0])?;
        let found = index.query(&[1.0, 0.0], 2)?;
        assert_eq!(found[0].chunk_id, "c3");
        Ok(())
    }

    #[test]
    fn lsh_ann_remove_and_reinsert_is_consistent() -> Result<()> {
        let mut index = LshAnnVectorIndex::new();
        index.upsert("c1", &[1.0, 0.0])?;
        index.upsert("c2", &[0.0, 1.0])?;
        index.remove("c1")?;
        index.upsert("c3", &[1.0, 0.0])?;
        let found = index.query(&[1.0, 0.0], 2)?;
        assert_eq!(found[0].chunk_id, "c3");
        Ok(())
    }
}
