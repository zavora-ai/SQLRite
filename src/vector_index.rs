use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::path::Path;
use std::pin::Pin;

use crate::{Result, SqlRiteError};
use hnsw_rs::api::AnnT;
use hnsw_rs::hnswio::HnswIo;
use hnsw_rs::prelude::{DistCosine, Hnsw};
use memmap2::{Mmap, MmapOptions};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorIndexMode {
    Disabled,
    BruteForce,
    LshAnn,
    HnswBaseline,
}

impl VectorIndexMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VectorIndexMode::Disabled => "disabled",
            VectorIndexMode::BruteForce => "brute_force",
            VectorIndexMode::LshAnn => "lsh_ann",
            VectorIndexMode::HnswBaseline => "hnsw_baseline",
        }
    }

    pub fn is_ann(self) -> bool {
        matches!(
            self,
            VectorIndexMode::LshAnn | VectorIndexMode::HnswBaseline
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VectorStorageKind {
    #[default]
    F32,
    F16,
    Int8,
}

impl VectorStorageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VectorStorageKind::F32 => "f32",
            VectorStorageKind::F16 => "f16",
            VectorStorageKind::Int8 => "int8",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AnnTuningConfig {
    pub min_candidates: usize,
    pub max_hamming_radius: usize,
    pub max_candidate_multiplier: usize,
}

impl Default for AnnTuningConfig {
    fn default() -> Self {
        Self {
            min_candidates: LSH_DEFAULT_MIN_CANDIDATES,
            max_hamming_radius: LSH_DEFAULT_MAX_HAMMING_RADIUS,
            max_candidate_multiplier: LSH_DEFAULT_MAX_CANDIDATE_MULTIPLIER,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VectorIndexOptions {
    pub storage_kind: VectorStorageKind,
    pub ann_tuning: AnnTuningConfig,
}

impl Default for VectorIndexOptions {
    fn default() -> Self {
        Self {
            storage_kind: VectorStorageKind::F32,
            ann_tuning: AnnTuningConfig::default(),
        }
    }
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

#[derive(Debug)]
pub(crate) enum BuiltinVectorIndex {
    BruteForce(BruteForceVectorIndex),
    LshAnn(LshAnnVectorIndex),
    HnswBaseline(HnswBaselineVectorIndex),
}

impl BuiltinVectorIndex {
    pub(crate) fn from_mode(mode: VectorIndexMode, options: VectorIndexOptions) -> Option<Self> {
        match mode {
            VectorIndexMode::Disabled => None,
            VectorIndexMode::BruteForce => Some(Self::BruteForce(
                BruteForceVectorIndex::new_with_storage(options.storage_kind),
            )),
            VectorIndexMode::LshAnn => Some(Self::LshAnn(LshAnnVectorIndex::new_with_options(
                options.storage_kind,
                options.ann_tuning,
            ))),
            VectorIndexMode::HnswBaseline => Some(Self::HnswBaseline(
                HnswBaselineVectorIndex::new_with_options(options.storage_kind, options.ann_tuning),
            )),
        }
    }

    pub(crate) fn storage_kind(&self) -> VectorStorageKind {
        match self {
            Self::BruteForce(index) => index.storage_kind,
            Self::LshAnn(index) => index.storage_kind,
            Self::HnswBaseline(index) => index.storage_kind(),
        }
    }

    pub(crate) fn export_entries(&self) -> Vec<(String, Vec<f32>)> {
        match self {
            Self::BruteForce(index) => index
                .chunk_ids
                .iter()
                .enumerate()
                .map(|(position, chunk_id)| {
                    let embedding = index.segments.embedding_vec(position);
                    (chunk_id.clone(), embedding)
                })
                .collect(),
            Self::LshAnn(index) => index
                .entries
                .iter()
                .map(|entry| (entry.chunk_id.clone(), entry.normalized_embedding.clone()))
                .collect(),
            Self::HnswBaseline(index) => index
                .chunk_ids
                .iter()
                .enumerate()
                .map(|(position, chunk_id)| {
                    let embedding = index.segments.embedding(position).to_vec();
                    (chunk_id.clone(), embedding)
                })
                .collect(),
        }
    }

    pub(crate) fn import_entries(&mut self, entries: &[(String, Vec<f32>)]) -> Result<()> {
        self.reset()?;
        let refs: Vec<(&str, &[f32])> = entries
            .iter()
            .map(|(chunk_id, embedding)| (chunk_id.as_str(), embedding.as_slice()))
            .collect();
        self.upsert_batch(&refs)
    }

    pub(crate) fn query_filtered(
        &self,
        query_embedding: &[f32],
        limit: usize,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<VectorCandidate>> {
        match self {
            Self::BruteForce(index) => index.query_filtered(query_embedding, limit, allowed_ids),
            Self::LshAnn(index) => index.query_filtered(query_embedding, limit, allowed_ids),
            Self::HnswBaseline(index) => index.query_filtered(query_embedding, limit, allowed_ids),
        }
    }

    #[cfg(test)]
    pub(crate) fn graph_ready(&self) -> bool {
        match self {
            Self::HnswBaseline(index) => index.graph_ready(),
            _ => false,
        }
    }
}

impl VectorIndex for BuiltinVectorIndex {
    fn name(&self) -> &'static str {
        match self {
            Self::BruteForce(index) => index.name(),
            Self::LshAnn(index) => index.name(),
            Self::HnswBaseline(index) => index.name(),
        }
    }

    fn dimension(&self) -> Option<usize> {
        match self {
            Self::BruteForce(index) => index.dimension(),
            Self::LshAnn(index) => index.dimension(),
            Self::HnswBaseline(index) => index.dimension(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::BruteForce(index) => index.len(),
            Self::LshAnn(index) => index.len(),
            Self::HnswBaseline(index) => index.len(),
        }
    }

    fn estimated_memory_bytes(&self) -> usize {
        match self {
            Self::BruteForce(index) => index.estimated_memory_bytes(),
            Self::LshAnn(index) => index.estimated_memory_bytes(),
            Self::HnswBaseline(index) => index.estimated_memory_bytes(),
        }
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.upsert(chunk_id, embedding),
            Self::LshAnn(index) => index.upsert(chunk_id, embedding),
            Self::HnswBaseline(index) => index.upsert(chunk_id, embedding),
        }
    }

    fn upsert_batch(&mut self, items: &[(&str, &[f32])]) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.upsert_batch(items),
            Self::LshAnn(index) => index.upsert_batch(items),
            Self::HnswBaseline(index) => index.upsert_batch(items),
        }
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.remove(chunk_id),
            Self::LshAnn(index) => index.remove(chunk_id),
            Self::HnswBaseline(index) => index.remove(chunk_id),
        }
    }

    fn reset(&mut self) -> Result<()> {
        match self {
            Self::BruteForce(index) => index.reset(),
            Self::LshAnn(index) => index.reset(),
            Self::HnswBaseline(index) => index.reset(),
        }
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        match self {
            Self::BruteForce(index) => index.query(query_embedding, limit),
            Self::LshAnn(index) => index.query(query_embedding, limit),
            Self::HnswBaseline(index) => index.query(query_embedding, limit),
        }
    }
}

#[derive(Debug, Clone)]
struct VectorEntry {
    chunk_id: String,
    normalized_embedding: Vec<f32>,
}

#[derive(Debug, Default)]
struct VectorSegmentStore {
    dimension: usize,
    values: Vec<f32>,
}

impl VectorSegmentStore {
    fn with_dimension(dimension: usize) -> Self {
        Self {
            dimension,
            values: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        if self.dimension == 0 {
            0
        } else {
            self.values.len() / self.dimension
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn reserve(&mut self, additional_embeddings: usize) {
        self.values
            .reserve(additional_embeddings.saturating_mul(self.dimension));
    }

    fn push(&mut self, embedding: &[f32]) {
        debug_assert_eq!(self.dimension, embedding.len());
        self.values.extend_from_slice(embedding);
    }

    fn set(&mut self, index: usize, embedding: &[f32]) {
        debug_assert_eq!(self.dimension, embedding.len());
        let start = index * self.dimension;
        let end = start + self.dimension;
        self.values[start..end].copy_from_slice(embedding);
    }

    fn embedding(&self, index: usize) -> &[f32] {
        let start = index * self.dimension;
        let end = start + self.dimension;
        &self.values[start..end]
    }

    fn swap_remove(&mut self, index: usize) {
        if self.is_empty() {
            return;
        }
        let last_index = self.len() - 1;
        if index != last_index {
            let dim = self.dimension;
            let start = index * dim;
            let last_start = last_index * dim;
            for offset in 0..dim {
                self.values[start + offset] = self.values[last_start + offset];
            }
        }
        self.values
            .truncate(self.values.len().saturating_sub(self.dimension));
    }
}

#[derive(Debug)]
struct MmapVectorSegmentStore {
    dimension: usize,
    mmap: Mmap,
    vector_offsets: Vec<usize>,
}

impl MmapVectorSegmentStore {
    fn load_f32_sidecar(path: &Path) -> Result<(Vec<String>, usize, Self)> {
        let file = std::fs::File::open(path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        let bytes = &mmap[..];
        if bytes.len() < 17 {
            return Err(SqlRiteError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "exact segment sidecar is too small",
            )));
        }
        if &bytes[..8] != b"SQLRSEG1" {
            return Err(SqlRiteError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid exact segment snapshot magic",
            )));
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().expect("slice has length"));
        if version != 1 {
            return Err(SqlRiteError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported exact segment snapshot version {version}"),
            )));
        }
        let storage_kind = bytes[12];
        if storage_kind != 1 {
            return Err(SqlRiteError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "mmap exact sidecar currently requires f32 storage",
            )));
        }
        let entry_count =
            u32::from_le_bytes(bytes[13..17].try_into().expect("slice has length")) as usize;
        let mut cursor = 17usize;
        let mut chunk_ids = Vec::with_capacity(entry_count);
        let mut vector_offsets = Vec::with_capacity(entry_count);
        let mut dimension = None;
        for _ in 0..entry_count {
            let id_len = read_u32_at(bytes, &mut cursor)? as usize;
            let id_bytes = read_bytes_at(bytes, &mut cursor, id_len)?;
            let chunk_id = String::from_utf8(id_bytes.to_vec()).map_err(|error| {
                SqlRiteError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                ))
            })?;
            let entry_dimension = read_u32_at(bytes, &mut cursor)? as usize;
            if let Some(expected_dimension) = dimension {
                if expected_dimension != entry_dimension {
                    return Err(SqlRiteError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "mixed dimensions in mmap exact sidecar",
                    )));
                }
            } else {
                dimension = Some(entry_dimension);
            }
            let vector_bytes = entry_dimension.saturating_mul(size_of::<f32>());
            if cursor + vector_bytes > bytes.len() {
                return Err(SqlRiteError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "unexpected EOF while parsing mmap exact sidecar",
                )));
            }
            chunk_ids.push(chunk_id);
            vector_offsets.push(cursor);
            cursor += vector_bytes;
        }

        Ok((
            chunk_ids,
            dimension.unwrap_or(0),
            Self {
                dimension: dimension.unwrap_or(0),
                mmap,
                vector_offsets,
            },
        ))
    }

    fn len(&self) -> usize {
        self.vector_offsets.len()
    }

    fn embedding(&self, index: usize) -> Vec<f32> {
        let offset = self.vector_offsets[index];
        let bytes = &self.mmap[offset..offset + self.dimension * size_of::<f32>()];
        let mut values = Vec::with_capacity(self.dimension);
        for chunk in bytes.chunks_exact(size_of::<f32>()) {
            values.push(f32::from_le_bytes(
                chunk.try_into().expect("chunk has exact f32 width"),
            ));
        }
        values
    }

    fn dot_product(&self, index: usize, query: &[f32]) -> f32 {
        let offset = self.vector_offsets[index];
        let bytes = &self.mmap[offset..offset + self.dimension * size_of::<f32>()];
        dot_product_f32_bytes(query, bytes)
    }

    fn to_owned_store(&self) -> Result<VectorSegmentStore> {
        let mut store = VectorSegmentStore::with_dimension(self.dimension);
        store.reserve(self.len());
        for position in 0..self.len() {
            let embedding = self.embedding(position);
            store.push(&embedding);
        }
        Ok(store)
    }
}

#[derive(Debug)]
enum SegmentStorage {
    Owned(VectorSegmentStore),
    Mapped(MmapVectorSegmentStore),
}

impl Default for SegmentStorage {
    fn default() -> Self {
        Self::Owned(VectorSegmentStore::default())
    }
}

impl SegmentStorage {
    fn embedding_vec(&self, index: usize) -> Vec<f32> {
        match self {
            Self::Owned(store) => store.embedding(index).to_vec(),
            Self::Mapped(store) => store.embedding(index),
        }
    }

    fn dot_product(&self, index: usize, query: &[f32]) -> f32 {
        match self {
            Self::Owned(store) => dot_product(query, store.embedding(index)),
            Self::Mapped(store) => store.dot_product(index, query),
        }
    }

    fn estimated_bytes(
        &self,
        storage_kind: VectorStorageKind,
        len: usize,
        dimension: Option<usize>,
    ) -> usize {
        match self {
            Self::Owned(_) => dimension
                .map(|dim| len * vector_storage_bytes(dim, storage_kind))
                .unwrap_or(0),
            Self::Mapped(store) => {
                store.mmap.len() + store.vector_offsets.len() * size_of::<usize>()
            }
        }
    }

    fn to_owned_store(&self) -> Result<VectorSegmentStore> {
        match self {
            Self::Owned(store) => Ok(VectorSegmentStore {
                dimension: store.dimension,
                values: store.values.clone(),
            }),
            Self::Mapped(store) => store.to_owned_store(),
        }
    }
}

#[derive(Debug, Default)]
pub struct BruteForceVectorIndex {
    storage_kind: VectorStorageKind,
    dimension: Option<usize>,
    chunk_ids: Vec<String>,
    segments: SegmentStorage,
    positions: HashMap<String, usize>,
}

const PARALLEL_SCAN_THRESHOLD: usize = 4_096;
const HNSW_GRAPH_LAYER_COUNT: usize = 16;
const LSH_DEFAULT_BITS_PER_TABLE: usize = 14;
const LSH_DEFAULT_TABLE_COUNT: usize = 6;
const LSH_DEFAULT_MIN_CANDIDATES: usize = 192;
const LSH_DEFAULT_MAX_HAMMING_RADIUS: usize = 2;
const LSH_DEFAULT_MAX_CANDIDATE_MULTIPLIER: usize = 8;
const LSH_PARALLEL_SCORE_THRESHOLD: usize = 2_048;
const BATCH_PARALLEL_PREP_THRESHOLD: usize = 512;

impl BruteForceVectorIndex {
    pub fn new() -> Self {
        Self::new_with_storage(VectorStorageKind::F32)
    }

    pub fn new_with_storage(storage_kind: VectorStorageKind) -> Self {
        Self {
            storage_kind,
            ..Self::default()
        }
    }

    pub fn load_mmap_f32_sidecar(path: &Path) -> Result<Self> {
        let (chunk_ids, dimension, mapped_store) = MmapVectorSegmentStore::load_f32_sidecar(path)?;
        let positions = chunk_ids
            .iter()
            .enumerate()
            .map(|(position, chunk_id)| (chunk_id.clone(), position))
            .collect();
        Ok(Self {
            storage_kind: VectorStorageKind::F32,
            dimension: if dimension == 0 {
                None
            } else {
                Some(dimension)
            },
            chunk_ids,
            segments: SegmentStorage::Mapped(mapped_store),
            positions,
        })
    }

    fn validate_dimension(&self, embedding: &[f32]) -> Result<()> {
        validate_dimension(self.dimension, embedding)
    }

    fn ensure_owned_segments(&mut self) -> Result<()> {
        if matches!(self.segments, SegmentStorage::Owned(_)) {
            return Ok(());
        }
        self.segments = SegmentStorage::Owned(self.segments.to_owned_store()?);
        Ok(())
    }

    fn query_filtered(
        &self,
        query_embedding: &[f32],
        limit: usize,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.chunk_ids.is_empty() || allowed_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;

        let query_normalized = normalize_embedding(query_embedding);
        let mut results: Vec<VectorCandidate> = self
            .chunk_ids
            .iter()
            .enumerate()
            .filter(|(_, chunk_id)| allowed_ids.contains(*chunk_id))
            .map(|(position, chunk_id)| VectorCandidate {
                chunk_id: chunk_id.clone(),
                score: self.segments.dot_product(position, &query_normalized),
            })
            .collect();

        if results.len() > limit {
            let nth = limit - 1;
            results.select_nth_unstable_by(nth, compare_candidates_desc);
            results.truncate(limit);
        }
        results.sort_by(compare_candidates_desc);
        Ok(results)
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
        self.chunk_ids.len()
    }

    fn estimated_memory_bytes(&self) -> usize {
        let embedding_bytes =
            self.segments
                .estimated_bytes(self.storage_kind, self.chunk_ids.len(), self.dimension);
        let id_bytes = self
            .chunk_ids
            .iter()
            .map(|chunk_id| chunk_id.len())
            .sum::<usize>();
        let positions_overhead =
            self.positions.len() * (size_of::<usize>() + size_of::<String>() + size_of::<usize>());
        embedding_bytes + id_bytes + positions_overhead
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        self.ensure_owned_segments()?;
        if self.dimension.is_none() {
            self.dimension = Some(embedding.len());
            self.segments =
                SegmentStorage::Owned(VectorSegmentStore::with_dimension(embedding.len()));
        }

        let normalized_embedding = normalize_embedding(embedding);
        if let Some(position) = self.positions.get(chunk_id).copied() {
            if let SegmentStorage::Owned(store) = &mut self.segments {
                store.set(position, &normalized_embedding);
            }
            return Ok(());
        }

        let position = self.chunk_ids.len();
        self.chunk_ids.push(chunk_id.to_string());
        if let SegmentStorage::Owned(store) = &mut self.segments {
            store.push(&normalized_embedding);
        }
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
        self.ensure_owned_segments()?;
        if self.dimension.is_none() {
            self.dimension = Some(items[0].1.len());
            self.segments =
                SegmentStorage::Owned(VectorSegmentStore::with_dimension(items[0].1.len()));
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

        self.chunk_ids.reserve(prepared.len());
        if let SegmentStorage::Owned(store) = &mut self.segments {
            store.reserve(prepared.len());
        }
        self.positions.reserve(prepared.len());
        for (chunk_id, normalized_embedding) in prepared {
            if let Some(position) = self.positions.get(&chunk_id).copied() {
                if let SegmentStorage::Owned(store) = &mut self.segments {
                    store.set(position, &normalized_embedding);
                }
            } else {
                let position = self.chunk_ids.len();
                self.chunk_ids.push(chunk_id.clone());
                if let SegmentStorage::Owned(store) = &mut self.segments {
                    store.push(&normalized_embedding);
                }
                self.positions.insert(chunk_id, position);
            }
        }

        Ok(())
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        self.ensure_owned_segments()?;
        let Some(position) = self.positions.remove(chunk_id) else {
            return Ok(());
        };

        self.chunk_ids.swap_remove(position);
        if let SegmentStorage::Owned(store) = &mut self.segments {
            store.swap_remove(position);
        }
        if position < self.chunk_ids.len() {
            let moved_id = self.chunk_ids[position].clone();
            self.positions.insert(moved_id, position);
        }

        if self.chunk_ids.is_empty() {
            self.dimension = None;
            self.segments = SegmentStorage::Owned(VectorSegmentStore::default());
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.chunk_ids.clear();
        self.positions.clear();
        self.dimension = None;
        self.segments = SegmentStorage::Owned(VectorSegmentStore::default());
        Ok(())
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;

        let query_normalized = normalize_embedding(query_embedding);
        let mut results: Vec<VectorCandidate> = if self.chunk_ids.len() >= PARALLEL_SCAN_THRESHOLD {
            (0..self.chunk_ids.len())
                .into_par_iter()
                .map(|position| VectorCandidate {
                    chunk_id: self.chunk_ids[position].clone(),
                    score: self.segments.dot_product(position, &query_normalized),
                })
                .collect()
        } else {
            self.chunk_ids
                .iter()
                .enumerate()
                .map(|(position, chunk_id)| VectorCandidate {
                    chunk_id: chunk_id.clone(),
                    score: self.segments.dot_product(position, &query_normalized),
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
    storage_kind: VectorStorageKind,
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
            storage_kind: VectorStorageKind::F32,
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

    pub fn new_with_options(storage_kind: VectorStorageKind, tuning: AnnTuningConfig) -> Self {
        Self {
            storage_kind,
            min_candidates: tuning.min_candidates.max(1),
            max_hamming_radius: tuning.max_hamming_radius,
            max_candidate_multiplier: tuning.max_candidate_multiplier.max(1),
            ..Self::default()
        }
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

    fn query_filtered(
        &self,
        query_embedding: &[f32],
        limit: usize,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<VectorCandidate>> {
        filtered_scan_entries(
            &self
                .entries
                .iter()
                .map(|entry| VectorEntry {
                    chunk_id: entry.chunk_id.clone(),
                    normalized_embedding: entry.normalized_embedding.clone(),
                })
                .collect::<Vec<_>>(),
            query_embedding,
            limit,
            allowed_ids,
        )
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
                    + vector_storage_bytes(entry.normalized_embedding.len(), self.storage_kind)
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

pub struct HnswBaselineVectorIndex {
    storage_kind: VectorStorageKind,
    dimension: Option<usize>,
    chunk_ids: Vec<String>,
    segments: VectorSegmentStore,
    positions: HashMap<String, usize>,
    m: usize,
    ef_construction: usize,
    ef_search: usize,
    graph: RefCell<Option<PersistedHnswGraph>>,
    graph_dirty: Cell<bool>,
}

impl std::fmt::Debug for HnswBaselineVectorIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HnswBaselineVectorIndex")
            .field("storage_kind", &self.storage_kind)
            .field("dimension", &self.dimension)
            .field("entries", &self.chunk_ids.len())
            .field("m", &self.m)
            .field("ef_construction", &self.ef_construction)
            .field("ef_search", &self.ef_search)
            .field("graph_dirty", &self.graph_dirty.get())
            .finish()
    }
}

struct PersistedHnswGraph {
    graph: Hnsw<'static, f32, DistCosine>,
    // The pinned reloader must outlive `graph` for reload-backed graphs.
    _reloader: Option<Pin<Box<HnswIo>>>,
}

impl PersistedHnswGraph {
    fn from_built(graph: Hnsw<'static, f32, DistCosine>) -> Self {
        Self {
            graph,
            _reloader: None,
        }
    }

    fn load(directory: &Path, basename: &str) -> Result<Self> {
        let mut reloader = Box::pin(HnswIo::new(directory, basename));
        // SAFETY: the returned HNSW may borrow data owned by the pinned reloader.
        // The reloader is heap-pinned and stored alongside the graph for the same lifetime.
        let graph = unsafe {
            let reloader_ref = Pin::as_mut(&mut reloader).get_unchecked_mut();
            std::mem::transmute::<Hnsw<'_, f32, DistCosine>, Hnsw<'static, f32, DistCosine>>(
                reloader_ref
                    .load_hnsw::<f32, DistCosine>()
                    .map_err(|error| SqlRiteError::Io(std::io::Error::other(error.to_string())))?,
            )
        };
        Ok(Self {
            graph,
            _reloader: Some(reloader),
        })
    }

    fn dump(&self, directory: &Path, basename: &str) -> Result<()> {
        self.graph
            .file_dump(directory, basename)
            .map_err(|error| SqlRiteError::Io(std::io::Error::other(error.to_string())))?;
        Ok(())
    }

    fn as_ref(&self) -> &Hnsw<'static, f32, DistCosine> {
        &self.graph
    }
}

impl HnswBaselineVectorIndex {
    pub fn new_with_options(storage_kind: VectorStorageKind, tuning: AnnTuningConfig) -> Self {
        let m = 16usize;
        let ef_construction = 64usize;
        let ef_search = tuning.min_candidates.max(64);
        Self {
            storage_kind,
            dimension: None,
            chunk_ids: Vec::new(),
            segments: VectorSegmentStore::default(),
            positions: HashMap::new(),
            m,
            ef_construction,
            ef_search,
            graph: RefCell::new(None),
            graph_dirty: Cell::new(true),
        }
    }

    fn storage_kind(&self) -> VectorStorageKind {
        self.storage_kind
    }

    fn validate_dimension(&self, embedding: &[f32]) -> Result<()> {
        validate_dimension(self.dimension, embedding)
    }

    fn mark_dirty(&self) {
        self.graph.borrow_mut().take();
        self.graph_dirty.set(true);
    }

    pub(crate) fn dump_graph_snapshot(&self, directory: &Path, basename: &str) -> Result<()> {
        self.ensure_graph()?;
        if let Some(graph) = self.graph.borrow().as_ref() {
            graph.dump(directory, basename)?;
        }
        Ok(())
    }

    pub(crate) fn load_graph_snapshot(&self, directory: &Path, basename: &str) -> Result<()> {
        if self.chunk_ids.is_empty() {
            self.graph.borrow_mut().take();
            self.graph_dirty.set(false);
            return Ok(());
        }
        let graph = PersistedHnswGraph::load(directory, basename)?;
        *self.graph.borrow_mut() = Some(graph);
        self.graph_dirty.set(false);
        Ok(())
    }

    #[cfg(test)]
    fn graph_ready(&self) -> bool {
        !self.graph_dirty.get() && self.graph.borrow().is_some()
    }

    fn ensure_graph(&self) -> Result<()> {
        if !self.graph_dirty.get() && self.graph.borrow().is_some() {
            return Ok(());
        }

        if self.chunk_ids.is_empty() {
            self.graph.borrow_mut().take();
            self.graph_dirty.set(false);
            return Ok(());
        }

        let mut graph = Hnsw::<f32, DistCosine>::new(
            self.m,
            self.chunk_ids.len(),
            HNSW_GRAPH_LAYER_COUNT,
            self.ef_construction,
            DistCosine {},
        );
        let data_with_id: Vec<(&[f32], usize)> = self
            .chunk_ids
            .iter()
            .enumerate()
            .map(|(idx, _)| (self.segments.embedding(idx), idx))
            .collect();
        graph.parallel_insert_slice(&data_with_id);
        graph.set_searching_mode(true);
        *self.graph.borrow_mut() = Some(PersistedHnswGraph::from_built(graph));
        self.graph_dirty.set(false);
        Ok(())
    }

    fn query_filtered(
        &self,
        query_embedding: &[f32],
        limit: usize,
        allowed_ids: &HashSet<String>,
    ) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.chunk_ids.is_empty() || allowed_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;
        self.ensure_graph()?;

        let mut allowed_positions = allowed_ids
            .iter()
            .filter_map(|chunk_id| self.positions.get(chunk_id).copied())
            .collect::<Vec<_>>();
        if allowed_positions.is_empty() {
            return Ok(Vec::new());
        }
        allowed_positions.sort_unstable();

        let query_normalized = normalize_embedding(query_embedding);
        let ef_search = self.ef_search.max(limit);
        let graph = self.graph.borrow();
        let Some(graph) = graph.as_ref() else {
            return Ok(Vec::new());
        };

        let mut results: Vec<VectorCandidate> = graph
            .as_ref()
            .search_filter(
                &query_normalized,
                limit,
                ef_search,
                Some(&allowed_positions),
            )
            .into_iter()
            .filter_map(|neighbor| {
                self.chunk_ids
                    .get(neighbor.d_id)
                    .map(|chunk_id| VectorCandidate {
                        chunk_id: chunk_id.clone(),
                        score: (1.0 - neighbor.distance).clamp(-1.0, 1.0),
                    })
            })
            .collect();

        results.sort_by(compare_candidates_desc);
        if results.len() > limit {
            results.truncate(limit);
        }
        Ok(results)
    }
}

impl VectorIndex for HnswBaselineVectorIndex {
    fn name(&self) -> &'static str {
        "hnsw_baseline"
    }

    fn dimension(&self) -> Option<usize> {
        self.dimension
    }

    fn len(&self) -> usize {
        self.chunk_ids.len()
    }

    fn estimated_memory_bytes(&self) -> usize {
        let embedding_bytes = self
            .dimension
            .map(|dim| self.chunk_ids.len() * vector_storage_bytes(dim, self.storage_kind))
            .unwrap_or(0);
        let id_bytes = self
            .chunk_ids
            .iter()
            .map(|chunk_id| chunk_id.len())
            .sum::<usize>();
        let positions_overhead =
            self.positions.len() * (size_of::<usize>() + size_of::<String>() + size_of::<usize>());
        let graph_link_budget = self.chunk_ids.len() * self.m * size_of::<usize>() * 2;
        embedding_bytes + id_bytes + positions_overhead + graph_link_budget
    }

    fn upsert(&mut self, chunk_id: &str, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        if self.dimension.is_none() {
            self.dimension = Some(embedding.len());
            self.segments = VectorSegmentStore::with_dimension(embedding.len());
        }

        let normalized_embedding = normalize_embedding(embedding);
        if let Some(position) = self.positions.get(chunk_id).copied() {
            self.segments.set(position, &normalized_embedding);
            self.mark_dirty();
            return Ok(());
        }

        let position = self.chunk_ids.len();
        self.chunk_ids.push(chunk_id.to_string());
        self.segments.push(&normalized_embedding);
        self.positions.insert(chunk_id.to_string(), position);
        self.mark_dirty();
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
            self.segments = VectorSegmentStore::with_dimension(items[0].1.len());
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

        self.chunk_ids.reserve(prepared.len());
        self.segments.reserve(prepared.len());
        self.positions.reserve(prepared.len());
        for (chunk_id, normalized_embedding) in prepared {
            if let Some(position) = self.positions.get(&chunk_id).copied() {
                self.segments.set(position, &normalized_embedding);
            } else {
                let position = self.chunk_ids.len();
                self.chunk_ids.push(chunk_id.clone());
                self.segments.push(&normalized_embedding);
                self.positions.insert(chunk_id, position);
            }
        }
        self.mark_dirty();
        Ok(())
    }

    fn remove(&mut self, chunk_id: &str) -> Result<()> {
        let Some(position) = self.positions.remove(chunk_id) else {
            return Ok(());
        };

        self.chunk_ids.swap_remove(position);
        self.segments.swap_remove(position);
        if position < self.chunk_ids.len() {
            let moved_id = self.chunk_ids[position].clone();
            self.positions.insert(moved_id, position);
        }

        if self.chunk_ids.is_empty() {
            self.dimension = None;
            self.segments = VectorSegmentStore::default();
            self.graph.borrow_mut().take();
            self.graph_dirty.set(false);
        } else {
            self.mark_dirty();
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.dimension = None;
        self.chunk_ids.clear();
        self.segments = VectorSegmentStore::default();
        self.positions.clear();
        self.graph.borrow_mut().take();
        self.graph_dirty.set(false);
        Ok(())
    }

    fn query(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<VectorCandidate>> {
        if limit == 0 || self.chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.validate_dimension(query_embedding)?;
        self.ensure_graph()?;

        let query_normalized = normalize_embedding(query_embedding);
        let ef_search = self.ef_search.max(limit);
        let graph = self.graph.borrow();
        let Some(graph) = graph.as_ref() else {
            return Ok(Vec::new());
        };

        let mut results: Vec<VectorCandidate> = graph
            .as_ref()
            .search(&query_normalized, limit, ef_search)
            .into_iter()
            .filter_map(|neighbor| {
                self.chunk_ids
                    .get(neighbor.d_id)
                    .map(|chunk_id| VectorCandidate {
                        chunk_id: chunk_id.clone(),
                        score: (1.0 - neighbor.distance).clamp(-1.0, 1.0),
                    })
            })
            .collect();

        results.sort_by(compare_candidates_desc);
        if results.len() > limit {
            results.truncate(limit);
        }
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

fn filtered_scan_entries(
    entries: &[VectorEntry],
    query_embedding: &[f32],
    limit: usize,
    allowed_ids: &HashSet<String>,
) -> Result<Vec<VectorCandidate>> {
    if limit == 0 || entries.is_empty() || allowed_ids.is_empty() {
        return Ok(Vec::new());
    }

    validate_dimension(Some(entries[0].normalized_embedding.len()), query_embedding)?;
    let query_normalized = normalize_embedding(query_embedding);
    let mut results: Vec<VectorCandidate> = entries
        .iter()
        .filter(|entry| allowed_ids.contains(&entry.chunk_id))
        .map(|entry| VectorCandidate {
            chunk_id: entry.chunk_id.clone(),
            score: dot_product(&query_normalized, &entry.normalized_embedding),
        })
        .collect();

    if results.len() > limit {
        let nth = limit - 1;
        results.select_nth_unstable_by(nth, compare_candidates_desc);
        results.truncate(limit);
    }
    results.sort_by(compare_candidates_desc);
    Ok(results)
}

fn vector_storage_bytes(dim: usize, storage_kind: VectorStorageKind) -> usize {
    match storage_kind {
        VectorStorageKind::F32 => dim * size_of::<f32>(),
        VectorStorageKind::F16 => dim * size_of::<u16>(),
        VectorStorageKind::Int8 => dim * size_of::<i8>() + size_of::<f32>(),
    }
}

fn read_u32_at(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    if *cursor + size_of::<u32>() > bytes.len() {
        return Err(SqlRiteError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "unexpected EOF while reading u32",
        )));
    }
    let value = u32::from_le_bytes(
        bytes[*cursor..*cursor + size_of::<u32>()]
            .try_into()
            .expect("slice has u32 length"),
    );
    *cursor += size_of::<u32>();
    Ok(value)
}

fn read_bytes_at<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    if *cursor + len > bytes.len() {
        return Err(SqlRiteError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "unexpected EOF while reading bytes",
        )));
    }
    let slice = &bytes[*cursor..*cursor + len];
    *cursor += len;
    Ok(slice)
}

fn normalize_embedding(embedding: &[f32]) -> Vec<f32> {
    let norm = l2_norm_unrolled(embedding);
    if norm == 0.0 {
        return embedding.to_vec();
    }
    embedding.iter().map(|value| value / norm).collect()
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return unsafe { dot_product_avx2(left, right) };
        }
    }
    dot_product_scalar(left, right)
}

fn dot_product_scalar(left: &[f32], right: &[f32]) -> f32 {
    let len = left.len().min(right.len());
    let mut acc0 = 0.0f32;
    let mut acc1 = 0.0f32;
    let mut acc2 = 0.0f32;
    let mut acc3 = 0.0f32;
    let mut i = 0usize;
    while i + 4 <= len {
        acc0 += left[i] * right[i];
        acc1 += left[i + 1] * right[i + 1];
        acc2 += left[i + 2] * right[i + 2];
        acc3 += left[i + 3] * right[i + 3];
        i += 4;
    }
    let mut tail = 0.0f32;
    while i < len {
        tail += left[i] * right[i];
        i += 1;
    }
    acc0 + acc1 + acc2 + acc3 + tail
}

fn dot_product_f32_bytes(left: &[f32], right_bytes: &[u8]) -> f32 {
    let available = right_bytes.len() / size_of::<f32>();
    let len = left.len().min(available);
    let mut acc0 = 0.0f32;
    let mut acc1 = 0.0f32;
    let mut acc2 = 0.0f32;
    let mut acc3 = 0.0f32;
    let mut i = 0usize;
    while i + 4 <= len {
        let right0 = f32::from_le_bytes(
            right_bytes[i * 4..i * 4 + 4]
                .try_into()
                .expect("slice has f32 width"),
        );
        let right1 = f32::from_le_bytes(
            right_bytes[(i + 1) * 4..(i + 1) * 4 + 4]
                .try_into()
                .expect("slice has f32 width"),
        );
        let right2 = f32::from_le_bytes(
            right_bytes[(i + 2) * 4..(i + 2) * 4 + 4]
                .try_into()
                .expect("slice has f32 width"),
        );
        let right3 = f32::from_le_bytes(
            right_bytes[(i + 3) * 4..(i + 3) * 4 + 4]
                .try_into()
                .expect("slice has f32 width"),
        );
        acc0 += left[i] * right0;
        acc1 += left[i + 1] * right1;
        acc2 += left[i + 2] * right2;
        acc3 += left[i + 3] * right3;
        i += 4;
    }
    let mut tail = 0.0f32;
    while i < len {
        let right = f32::from_le_bytes(
            right_bytes[i * 4..i * 4 + 4]
                .try_into()
                .expect("slice has f32 width"),
        );
        tail += left[i] * right;
        i += 1;
    }
    acc0 + acc1 + acc2 + acc3 + tail
}

fn l2_norm_unrolled(values: &[f32]) -> f32 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return unsafe { l2_norm_avx2(values) };
        }
    }
    l2_norm_scalar(values)
}

fn l2_norm_scalar(values: &[f32]) -> f32 {
    let mut acc0 = 0.0f32;
    let mut acc1 = 0.0f32;
    let mut acc2 = 0.0f32;
    let mut acc3 = 0.0f32;
    let mut i = 0usize;
    while i + 4 <= values.len() {
        acc0 += values[i] * values[i];
        acc1 += values[i + 1] * values[i + 1];
        acc2 += values[i + 2] * values[i + 2];
        acc3 += values[i + 3] * values[i + 3];
        i += 4;
    }
    let mut tail = 0.0f32;
    while i < values.len() {
        tail += values[i] * values[i];
        i += 1;
    }
    (acc0 + acc1 + acc2 + acc3 + tail).sqrt()
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn dot_product_avx2(left: &[f32], right: &[f32]) -> f32 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{
        __m256, _mm256_add_ps, _mm256_loadu_ps, _mm256_mul_ps, _mm256_setzero_ps, _mm256_storeu_ps,
    };
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        __m256, _mm256_add_ps, _mm256_loadu_ps, _mm256_mul_ps, _mm256_setzero_ps, _mm256_storeu_ps,
    };

    let len = left.len().min(right.len());
    let mut i = 0usize;
    let mut acc: __m256 = _mm256_setzero_ps();
    while i + 8 <= len {
        let left_vec = _mm256_loadu_ps(left.as_ptr().add(i));
        let right_vec = _mm256_loadu_ps(right.as_ptr().add(i));
        acc = _mm256_add_ps(acc, _mm256_mul_ps(left_vec, right_vec));
        i += 8;
    }

    let mut lanes = [0.0f32; 8];
    _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
    let mut total = lanes.iter().sum::<f32>();
    while i < len {
        total += left[i] * right[i];
        i += 1;
    }
    total
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn l2_norm_avx2(values: &[f32]) -> f32 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{
        __m256, _mm256_add_ps, _mm256_loadu_ps, _mm256_mul_ps, _mm256_setzero_ps, _mm256_storeu_ps,
    };
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        __m256, _mm256_add_ps, _mm256_loadu_ps, _mm256_mul_ps, _mm256_setzero_ps, _mm256_storeu_ps,
    };

    let mut i = 0usize;
    let mut acc: __m256 = _mm256_setzero_ps();
    while i + 8 <= values.len() {
        let vec = _mm256_loadu_ps(values.as_ptr().add(i));
        acc = _mm256_add_ps(acc, _mm256_mul_ps(vec, vec));
        i += 8;
    }

    let mut lanes = [0.0f32; 8];
    _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
    let mut total = lanes.iter().sum::<f32>();
    while i < values.len() {
        total += values[i] * values[i];
        i += 1;
    }
    total.sqrt()
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
    use tempfile::tempdir;

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
    fn hnsw_baseline_queries_by_similarity() -> Result<()> {
        run_index_contract(HnswBaselineVectorIndex::new_with_options(
            VectorStorageKind::F32,
            AnnTuningConfig::default(),
        ))
    }

    #[test]
    fn hnsw_baseline_query_filtered_respects_allow_list() -> Result<()> {
        let mut index =
            HnswBaselineVectorIndex::new_with_options(VectorStorageKind::F32, Default::default());
        index.upsert("acme-top", &[1.0, 0.0, 0.0])?;
        index.upsert("beta-top", &[0.99, 0.01, 0.0])?;
        index.upsert("beta-second", &[0.95, 0.05, 0.0])?;

        let allowed = HashSet::from(["beta-top".to_string(), "beta-second".to_string()]);
        let found = index.query_filtered(&[1.0, 0.0, 0.0], 1, &allowed)?;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].chunk_id, "beta-top");
        Ok(())
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
    fn hnsw_baseline_rejects_dimension_mismatch() -> Result<()> {
        let mut index =
            HnswBaselineVectorIndex::new_with_options(VectorStorageKind::F32, Default::default());
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

    #[test]
    fn hnsw_baseline_remove_and_reinsert_is_consistent() -> Result<()> {
        let mut index =
            HnswBaselineVectorIndex::new_with_options(VectorStorageKind::F32, Default::default());
        index.upsert("c1", &[1.0, 0.0])?;
        index.upsert("c2", &[0.0, 1.0])?;
        index.remove("c1")?;
        index.upsert("c3", &[1.0, 0.0])?;
        let found = index.query(&[1.0, 0.0], 2)?;
        assert_eq!(found[0].chunk_id, "c3");
        Ok(())
    }

    #[test]
    fn storage_kind_changes_memory_estimate() -> Result<()> {
        let mut f32_index = BruteForceVectorIndex::new_with_storage(VectorStorageKind::F32);
        let mut f16_index = BruteForceVectorIndex::new_with_storage(VectorStorageKind::F16);
        let mut int8_index = BruteForceVectorIndex::new_with_storage(VectorStorageKind::Int8);

        for index in [&mut f32_index, &mut f16_index, &mut int8_index] {
            index.upsert("v1", &[0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5])?;
        }

        assert!(
            f32_index.estimated_memory_bytes() > f16_index.estimated_memory_bytes(),
            "expected f32 estimate > f16 estimate"
        );
        assert!(
            f16_index.estimated_memory_bytes() > int8_index.estimated_memory_bytes(),
            "expected f16 estimate > int8 estimate"
        );
        Ok(())
    }

    #[test]
    fn brute_force_can_load_mmap_sidecar() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("exact-sidecar-f32.bin");
        let entries = [
            ("c1", vec![1.0f32, 0.0, 0.0]),
            ("c2", vec![0.0f32, 1.0, 0.0]),
            ("c3", vec![0.8f32, 0.2, 0.0]),
        ];

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SQLRSEG1");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.push(1u8);
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (chunk_id, embedding) in entries {
            bytes.extend_from_slice(&(chunk_id.len() as u32).to_le_bytes());
            bytes.extend_from_slice(chunk_id.as_bytes());
            bytes.extend_from_slice(&(embedding.len() as u32).to_le_bytes());
            for value in embedding {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        std::fs::write(&path, bytes)?;

        let index = BruteForceVectorIndex::load_mmap_f32_sidecar(&path)?;
        let found = index.query(&[0.9, 0.1, 0.0], 2)?;
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].chunk_id, "c1");
        Ok(())
    }
}
