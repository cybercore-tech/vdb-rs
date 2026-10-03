//! Versioned mmap-backed HNSW graph files. Indexes are disposable derived data.
use crate::error::{Error, Result};
use memmap2::Mmap;
use std::fs::File;
#[cfg(all(feature = "metrics", feature = "serde-query"))]
use std::io::Write;
use std::path::Path;

/// Graph-file magic (format version 2).
pub const MAGIC: &[u8; 4] = b"IDX2";
const HEADER_LEN: usize = 64;
const MAX_LEVEL: usize = 16;

fn invalid(message: &str) -> Error {
    Error::Corrupt(format!("HNSW: {message}"))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| invalid("truncated u32"))?
            .try_into()
            .unwrap(),
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("truncated u64"))?
            .try_into()
            .unwrap(),
    ))
}

/// Validated read-only mmap of an HNSW graph, including its node directory.
pub struct IndexFile {
    mmap: Mmap,
    offsets: Vec<usize>,
    m: u32,
    ef_construction: u32,
    dim: u32,
    generation: u64,
    revision: u64,
    entry: usize,
    max_level: usize,
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    live: Option<std::collections::HashSet<u64>>,
}
impl IndexFile {
    /// Open and validate every node, layer, edge and file boundary.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // Safety: database queries hold the shared operation lock, and index
        // publication replaces files rather than modifying existing mappings.
        // Standalone callers must likewise exclude external file mutation.
        let mmap = unsafe { Mmap::map(&file)? };
        if mmap.len() < HEADER_LEN || &mmap[..4] != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                got: mmap.get(..4).unwrap_or_default().to_vec(),
            });
        }
        if mmap[4] != 0 || mmap[5..8] != [0; 3] {
            return Err(invalid("unsupported type or reserved fields"));
        }
        let expected_checksum =
            crate::checksum::checksum(&mmap[..56]) ^ crate::checksum::checksum(&mmap[64..]);
        if u64_at(&mmap, 56)? != expected_checksum {
            return Err(invalid("checksum"));
        }
        let m = u32_at(&mmap, 8)?;
        let ef_construction = u32_at(&mmap, 12)?;
        let dim = u32_at(&mmap, 16)?;
        crate::vector::validate_dimension(dim).map_err(|_| invalid("dimension"))?;
        let max_level = u32_at(&mmap, 20)? as usize;
        if !(2..=128).contains(&m) || ef_construction < m || max_level > MAX_LEVEL {
            return Err(invalid("parameters"));
        }
        let generation = u64_at(&mmap, 24)?;
        let revision = u64_at(&mmap, 32)?;
        let count = usize::try_from(u64_at(&mmap, 40)?).map_err(|_| invalid("node count"))?;
        let entry_raw = u64_at(&mmap, 48)?;
        let entry = if count == 0 {
            0
        } else {
            usize::try_from(entry_raw).map_err(|_| invalid("entry point"))?
        };
        if (count > 0 && entry >= count)
            || (count == 0 && (entry_raw != u64::MAX || max_level != 0))
        {
            return Err(invalid("entry point"));
        }
        let directory_end = count
            .checked_mul(8)
            .and_then(|n| n.checked_add(HEADER_LEN))
            .ok_or_else(|| invalid("directory overflow"))?;
        if directory_end > mmap.len() {
            return Err(invalid("truncated directory"));
        }
        let mut offsets = Vec::with_capacity(count);
        let mut levels = Vec::with_capacity(count);
        let mut expected = directory_end;
        let mut ids = std::collections::HashSet::new();
        for i in 0..count {
            let offset = usize::try_from(u64_at(&mmap, HEADER_LEN + i * 8)?)
                .map_err(|_| invalid("node offset"))?;
            if offset != expected {
                return Err(invalid("node directory"));
            }
            let id = u64_at(&mmap, offset)?;
            if !ids.insert(id) {
                return Err(invalid("duplicate ID"));
            }
            let vector_offset = u64_at(&mmap, offset + 8)?;
            let stride = crate::vector::block_len(dim) as u64;
            if vector_offset < 32 || !(vector_offset - 32).is_multiple_of(stride) {
                return Err(invalid("vector offset"));
            }
            let level = u32_at(&mmap, offset + 16)? as usize;
            if level > max_level {
                return Err(invalid("node level"));
            }
            expected = offset
                .checked_add(20)
                .ok_or_else(|| invalid("node overflow"))?;
            for layer in 0..=level {
                let degree = u32_at(&mmap, expected)? as usize;
                if degree > m as usize * if layer == 0 { 2 } else { 1 } {
                    return Err(invalid("degree"));
                }
                expected = expected
                    .checked_add(4 + degree * 8)
                    .ok_or_else(|| invalid("edge overflow"))?;
                if expected > mmap.len() {
                    return Err(invalid("truncated edges"));
                }
                for edge in 0..degree {
                    if u64_at(&mmap, expected - degree * 8 + edge * 8)? >= count as u64 {
                        return Err(invalid("edge target outside directory"));
                    }
                }
            }
            offsets.push(offset);
            levels.push(level);
        }
        if expected != mmap.len() {
            return Err(invalid("trailing data"));
        }
        let result = Self {
            mmap,
            offsets,
            m,
            ef_construction,
            dim,
            generation,
            revision,
            entry,
            max_level,
            #[cfg(all(feature = "metrics", feature = "serde-query"))]
            live: None,
        };
        if count > 0 && levels[entry] != max_level {
            return Err(invalid("entry level"));
        }
        for i in 0..count {
            for layer in 0..=levels[i] {
                let mut seen = std::collections::HashSet::new();
                for neighbor in result.neighbors(i, layer) {
                    if neighbor >= count
                        || neighbor == i
                        || levels[neighbor] < layer
                        || !seen.insert(neighbor)
                    {
                        return Err(invalid("edge target"));
                    }
                }
            }
        }
        Ok(result)
    }
    /// Maximum neighbors above layer zero.
    pub fn m(&self) -> u32 {
        self.m
    }
    /// Construction candidate budget.
    pub fn ef_construction(&self) -> u32 {
        self.ef_construction
    }
    /// Indexed vector dimension.
    pub fn dim(&self) -> u32 {
        self.dim
    }
    /// Raw graph file bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
    /// Number of indexed vectors.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
    /// Collection incarnation captured by this snapshot.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Mutation revision captured by this snapshot.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Highest graph layer.
    pub fn max_level(&self) -> usize {
        self.max_level
    }
    /// Entry node position, or none for an empty graph.
    pub fn entry_point(&self) -> Option<usize> {
        if self.is_empty() {
            None
        } else {
            Some(self.entry)
        }
    }
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    pub(crate) fn matches(&self, generation: u64, revision: u64, dim: u32) -> bool {
        self.generation == generation && self.revision == revision && self.dim == dim
    }
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    fn id(&self, node: usize) -> u64 {
        u64_at(&self.mmap, self.offsets[node]).unwrap()
    }
    #[cfg(all(feature = "metrics", feature = "serde-query"))]
    fn vector_offset(&self, node: usize) -> u64 {
        u64_at(&self.mmap, self.offsets[node] + 8).unwrap()
    }
    fn neighbors(&self, node: usize, layer: usize) -> Vec<usize> {
        let offset = self.offsets[node];
        if layer > u32_at(&self.mmap, offset + 16).unwrap() as usize {
            return Vec::new();
        }
        let mut cursor = offset + 20;
        for l in 0..=layer {
            let degree = u32_at(&self.mmap, cursor).unwrap() as usize;
            cursor += 4;
            if l == layer {
                return (0..degree)
                    .map(|j| u64_at(&self.mmap, cursor + j * 8).unwrap() as usize)
                    .collect();
            }
            cursor += degree * 8;
        }
        Vec::new()
    }
}

#[cfg(all(feature = "metrics", feature = "serde-query"))]
mod graph {
    use super::*;
    use crate::metric::Metric;
    use crate::vector::VectorFile;
    use std::cmp::{Ordering, Reverse};
    use std::collections::{BinaryHeap, HashSet};

    #[derive(Clone, Copy)]
    struct Rank {
        distance: f32,
        node: usize,
    }
    impl PartialEq for Rank {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other) == Ordering::Equal
        }
    }
    impl Eq for Rank {}
    impl PartialOrd for Rank {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Rank {
        fn cmp(&self, other: &Self) -> Ordering {
            self.distance
                .total_cmp(&other.distance)
                .then(self.node.cmp(&other.node))
        }
    }
    fn distance(metric: Metric, query: &[f32], vector: &[f32]) -> f32 {
        let score = metric.distance(query, vector);
        if metric.higher_is_better() {
            -score
        } else {
            score
        }
    }
    fn layer_search(
        entries: &[usize],
        ef: usize,
        layer: usize,
        mut score: impl FnMut(usize) -> Result<f32>,
        mut neighbors: impl FnMut(usize, usize) -> Vec<usize>,
    ) -> Result<Vec<usize>> {
        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut best = BinaryHeap::new();
        for &node in entries {
            let rank = Rank {
                distance: score(node)?,
                node,
            };
            if visited.insert(node) {
                candidates.push(Reverse(rank));
                best.push(rank);
            }
        }
        while let Some(Reverse(candidate)) = candidates.pop() {
            if best.len() >= ef && candidate > *best.peek().unwrap() {
                break;
            }
            for node in neighbors(candidate.node, layer) {
                if !visited.insert(node) {
                    continue;
                }
                let rank = Rank {
                    distance: score(node)?,
                    node,
                };
                if best.len() < ef || rank < *best.peek().unwrap() {
                    candidates.push(Reverse(rank));
                    best.push(rank);
                    if best.len() > ef {
                        best.pop();
                    }
                }
            }
        }
        let mut ranks = best.into_vec();
        ranks.sort();
        Ok(ranks.into_iter().map(|r| r.node).collect())
    }

    struct Node {
        id: u64,
        offset: u64,
        vector: Vec<f32>,
        links: Vec<Vec<usize>>,
    }
    /// Mutable construction graph. Queries traverse the published mmap graph.
    pub(crate) struct Hnsw {
        nodes: Vec<Node>,
        entry: usize,
        max_level: usize,
        m: usize,
        ef: usize,
    }
    impl Hnsw {
        pub(crate) fn build(vectors: Vec<(u64, u64, Vec<f32>)>, metric: Metric) -> Result<Self> {
            let mut graph = Self {
                nodes: Vec::new(),
                entry: 0,
                max_level: 0,
                m: 16,
                ef: 128,
            };
            for (id, offset, vector) in vectors {
                graph.insert(id, offset, vector, metric)?;
            }
            Ok(graph)
        }
        pub(crate) fn restore(
            index: &IndexFile,
            vectors: &VectorFile,
            metas: &[(u64, crate::kv::VectorMeta)],
        ) -> Result<Self> {
            let mut nodes = Vec::with_capacity(index.len());
            let mut offsets = std::collections::HashMap::new();
            for i in 0..index.len() {
                let offset = index.vector_offset(i);
                let level = u32_at(&index.mmap, index.offsets[i] + 16)? as usize;
                offsets.insert(index.id(i), offset);
                nodes.push(Node {
                    id: index.id(i),
                    offset,
                    vector: vectors.read_at(offset)?.to_vec(),
                    links: (0..=level).map(|layer| index.neighbors(i, layer)).collect(),
                });
            }
            for (id, meta) in metas {
                if offsets.get(id).is_some_and(|offset| *offset != meta.offset) {
                    return Err(invalid("snapshot vector location changed"));
                }
            }
            Ok(Self {
                nodes,
                entry: index.entry,
                max_level: index.max_level,
                m: index.m as usize,
                ef: index.ef_construction as usize,
            })
        }

        pub(crate) fn extend(
            &mut self,
            metas: &[(u64, crate::kv::VectorMeta)],
            vectors: &VectorFile,
            metric: Metric,
        ) -> Result<()> {
            let known: HashSet<_> = self.nodes.iter().map(|node| node.id).collect();
            for (id, meta) in metas {
                if !known.contains(id) {
                    self.insert(
                        *id,
                        meta.offset,
                        vectors.read_at(meta.offset)?.to_vec(),
                        metric,
                    )?;
                }
            }
            Ok(())
        }

        fn select(
            &self,
            query: &[f32],
            candidates: &[usize],
            limit: usize,
            metric: Metric,
        ) -> Vec<usize> {
            let mut selected = Vec::new();
            let mut rejected = Vec::new();
            for &candidate in candidates {
                let query_distance = distance(metric, query, &self.nodes[candidate].vector);
                if selected.iter().all(|&other: &usize| {
                    distance(
                        metric,
                        &self.nodes[candidate].vector,
                        &self.nodes[other].vector,
                    ) >= query_distance
                }) {
                    selected.push(candidate);
                    if selected.len() == limit {
                        return selected;
                    }
                } else {
                    rejected.push(candidate);
                }
            }
            selected.extend(rejected.into_iter().take(limit - selected.len()));
            selected
        }
        fn insert(&mut self, id: u64, offset: u64, vector: Vec<f32>, metric: Metric) -> Result<()> {
            // SplitMix64 gives reproducible geometric levels without a RNG dependency.
            let mut hash = id.wrapping_add(0x9e3779b97f4a7c15);
            hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
            hash ^= hash >> 31;
            let mut level = 0;
            while level < MAX_LEVEL && hash & 15 == 0 {
                level += 1;
                hash >>= 4;
            }
            let new = self.nodes.len();
            if new == 0 {
                self.nodes.push(Node {
                    id,
                    offset,
                    vector,
                    links: vec![Vec::new(); level + 1],
                });
                self.max_level = level;
                return Ok(());
            }
            let mut entry = self.entry;
            if level < self.max_level {
                for layer in ((level + 1)..=self.max_level).rev() {
                    entry = layer_search(
                        &[entry],
                        1,
                        layer,
                        |i| Ok(distance(metric, &vector, &self.nodes[i].vector)),
                        |i, l| self.nodes[i].links.get(l).cloned().unwrap_or_default(),
                    )?[0];
                }
            }
            let mut links = vec![Vec::new(); level + 1];
            for layer in (0..=level.min(self.max_level)).rev() {
                let candidates = layer_search(
                    &[entry],
                    self.ef,
                    layer,
                    |i| Ok(distance(metric, &vector, &self.nodes[i].vector)),
                    |i, l| self.nodes[i].links.get(l).cloned().unwrap_or_default(),
                )?;
                let limit = self.m * if layer == 0 { 2 } else { 1 };
                links[layer] = self.select(&vector, &candidates, limit, metric);
                entry = candidates[0];
            }
            self.nodes.push(Node {
                id,
                offset,
                vector,
                links,
            });
            for layer in 0..=level.min(self.max_level) {
                let neighbors = self.nodes[new].links[layer].clone();
                for neighbor in neighbors {
                    self.nodes[neighbor].links[layer].push(new);
                    let limit = self.m * if layer == 0 { 2 } else { 1 };
                    if self.nodes[neighbor].links[layer].len() > limit {
                        let mut candidates = self.nodes[neighbor].links[layer].clone();
                        candidates.sort_by(|&a, &b| {
                            distance(metric, &self.nodes[neighbor].vector, &self.nodes[a].vector)
                                .total_cmp(&distance(
                                    metric,
                                    &self.nodes[neighbor].vector,
                                    &self.nodes[b].vector,
                                ))
                                .then(a.cmp(&b))
                        });
                        self.nodes[neighbor].links[layer] =
                            self.select(&self.nodes[neighbor].vector, &candidates, limit, metric);
                    }
                }
            }
            if level > self.max_level {
                self.entry = new;
                self.max_level = level;
            }
            Ok(())
        }
        pub(crate) fn write(
            &self,
            path: &Path,
            dim: u32,
            generation: u64,
            revision: u64,
        ) -> Result<()> {
            let mut bytes = vec![0u8; HEADER_LEN + self.nodes.len() * 8];
            bytes[..4].copy_from_slice(MAGIC);
            bytes[8..12].copy_from_slice(&(self.m as u32).to_le_bytes());
            bytes[12..16].copy_from_slice(&(self.ef as u32).to_le_bytes());
            bytes[16..20].copy_from_slice(&dim.to_le_bytes());
            bytes[20..24].copy_from_slice(&(self.max_level as u32).to_le_bytes());
            bytes[24..32].copy_from_slice(&generation.to_le_bytes());
            bytes[32..40].copy_from_slice(&revision.to_le_bytes());
            bytes[40..48].copy_from_slice(&(self.nodes.len() as u64).to_le_bytes());
            bytes[48..56].copy_from_slice(
                &(if self.nodes.is_empty() {
                    u64::MAX
                } else {
                    self.entry as u64
                })
                .to_le_bytes(),
            );
            for (i, node) in self.nodes.iter().enumerate() {
                let offset = bytes.len() as u64;
                bytes[HEADER_LEN + i * 8..HEADER_LEN + i * 8 + 8]
                    .copy_from_slice(&offset.to_le_bytes());
                bytes.extend(node.id.to_le_bytes());
                bytes.extend(node.offset.to_le_bytes());
                bytes.extend((node.links.len() as u32 - 1).to_le_bytes());
                for layer in &node.links {
                    bytes.extend((layer.len() as u32).to_le_bytes());
                    for &neighbor in layer {
                        bytes.extend((neighbor as u64).to_le_bytes());
                    }
                }
            }
            let checksum =
                crate::checksum::checksum(&bytes[..56]) ^ crate::checksum::checksum(&bytes[64..]);
            bytes[56..64].copy_from_slice(&checksum.to_le_bytes());
            let mut file = File::create(path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            Ok(())
        }
    }
    impl IndexFile {
        pub(crate) fn set_live(&mut self, live: HashSet<u64>) {
            self.live = Some(live);
        }
        pub(crate) fn search(
            &self,
            vectors: &VectorFile,
            query: &[f32],
            k: usize,
            ef: usize,
            metric: Metric,
        ) -> Result<Vec<(u64, f32)>> {
            if self.is_empty() {
                return Ok(Vec::new());
            }
            let mut entry = self.entry;
            for layer in (1..=self.max_level).rev() {
                entry = layer_search(
                    &[entry],
                    1,
                    layer,
                    |i| {
                        Ok(distance(
                            metric,
                            query,
                            vectors.read_at(self.vector_offset(i))?,
                        ))
                    },
                    |i, l| self.neighbors(i, l),
                )?[0];
            }
            let candidates = layer_search(
                &[entry],
                // Deleted nodes can occupy beam slots but remain useful routes.
                // Widen by their count before excluding them from results.
                ef.max(k)
                    .saturating_add(
                        self.len()
                            .saturating_sub(self.live.as_ref().map_or(self.len(), HashSet::len)),
                    )
                    .min(self.len()),
                0,
                |i| {
                    Ok(distance(
                        metric,
                        query,
                        vectors.read_at(self.vector_offset(i))?,
                    ))
                },
                |i, l| self.neighbors(i, l),
            )?;
            candidates
                .into_iter()
                .filter(|i| {
                    self.live
                        .as_ref()
                        .is_none_or(|live| live.contains(&self.id(*i)))
                })
                .take(k)
                .map(|i| {
                    Ok((
                        self.id(i),
                        metric.distance(query, vectors.read_at(self.vector_offset(i))?),
                    ))
                })
                .collect()
        }
    }
}
#[cfg(all(feature = "metrics", feature = "serde-query"))]
pub(crate) use graph::Hnsw;
