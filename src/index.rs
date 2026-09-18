//! `.index` file: per-collection mmap'd HNSW graph. Layout: magic
//! `b"IDX1"`, `index_type: u8`, `m: u32`, `ef_construction: u32`,
//! `dim: u32`, then node/levels blocks. See
//! `docs/adr/0002-hnsw-on-disk-layout.md`.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

use crate::error::{Error, Result};

/// Magic bytes identifying an `.index` file.
pub const MAGIC: &[u8; 4] = b"IDX1";

const HEADER_LEN: usize = 4 + 1 + 4 + 4 + 4; // magic + type + m + ef_construction + dim

/// A read-only, mmap'd HNSW `.index` file for one collection.
pub struct IndexFile {
    mmap: Mmap,
    m: u32,
    ef_construction: u32,
    dim: u32,
}

impl IndexFile {
    /// Open an existing `.index` file and mmap it.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // Safety: see the invariant recorded in ADR-0001 / ADR-0002.
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < HEADER_LEN || &mmap[0..4] != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                got: mmap.get(0..4).unwrap_or_default().to_vec(),
            });
        }
        let m = u32::from_le_bytes(mmap[5..9].try_into().unwrap());
        let ef_construction = u32::from_le_bytes(mmap[9..13].try_into().unwrap());
        let dim = u32::from_le_bytes(mmap[13..17].try_into().unwrap());

        Ok(Self {
            mmap,
            m,
            ef_construction,
            dim,
        })
    }

    /// The `M` (max neighbors per node) HNSW parameter this index was built with.
    pub fn m(&self) -> u32 {
        self.m
    }

    /// The `ef_construction` HNSW parameter this index was built with.
    pub fn ef_construction(&self) -> u32 {
        self.ef_construction
    }

    /// The vector dimensionality this index was built for.
    pub fn dim(&self) -> u32 {
        self.dim
    }

    /// The raw mmap'd bytes (header + nodes + levels).
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_header(m: u32, ef_construction: u32, dim: u32) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(MAGIC).unwrap();
        f.write_all(&[0u8]).unwrap(); // index_type: 0 = HNSW
        f.write_all(&m.to_le_bytes()).unwrap();
        f.write_all(&ef_construction.to_le_bytes()).unwrap();
        f.write_all(&dim.to_le_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn open_reads_hnsw_params_from_a_valid_header() {
        let f = write_header(16, 200, 768);
        let idx = IndexFile::open(f.path()).unwrap();
        assert_eq!(idx.m(), 16);
        assert_eq!(idx.ef_construction(), 200);
        assert_eq!(idx.dim(), 768);
    }

    #[test]
    fn open_rejects_a_file_with_the_wrong_magic() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"NOPE").unwrap();
        f.flush().unwrap();
        assert!(matches!(
            IndexFile::open(f.path()),
            Err(Error::BadMagic { .. })
        ));
    }
}
