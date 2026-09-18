//! `.vectors` file: per-collection mmap'd, contiguous, 32-byte-aligned
//! `f32` arrays. Layout: magic `b"VDB1"`, `dim: u32`, `count: u64`, then
//! data. See `docs/storage_layout.md`.

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

use crate::error::{Error, Result};

/// Magic bytes identifying a `.vectors` file.
pub const MAGIC: &[u8; 4] = b"VDB1";

const HEADER_LEN: usize = 4 + 4 + 8; // magic + dim + count

/// A read-only, mmap'd `.vectors` file for one collection.
pub struct VectorFile {
    mmap: Mmap,
    dim: u32,
}

impl VectorFile {
    /// Open an existing `.vectors` file and mmap it.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // Safety: the file is not expected to be modified by another
        // process while mapped — see the invariant recorded in ADR-0001.
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < HEADER_LEN || &mmap[0..4] != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                got: mmap.get(0..4).unwrap_or_default().to_vec(),
            });
        }
        let dim = u32::from_le_bytes(mmap[4..8].try_into().unwrap());

        Ok(Self { mmap, dim })
    }

    /// The vector dimensionality this file was written with.
    pub fn dim(&self) -> u32 {
        self.dim
    }

    /// The number of vectors recorded in the header.
    pub fn count(&self) -> u64 {
        u64::from_le_bytes(self.mmap[8..16].try_into().unwrap())
    }

    /// The raw mmap'd bytes (header + data).
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_header(dim: u32, count: u64) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(MAGIC).unwrap();
        f.write_all(&dim.to_le_bytes()).unwrap();
        f.write_all(&count.to_le_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn open_reads_dimension_and_count_from_a_valid_header() {
        let f = write_header(768, 42);
        let vf = VectorFile::open(f.path()).unwrap();
        assert_eq!(vf.dim(), 768);
        assert_eq!(vf.count(), 42);
    }

    #[test]
    fn open_rejects_a_file_with_the_wrong_magic() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"NOPE").unwrap();
        f.flush().unwrap();
        assert!(matches!(
            VectorFile::open(f.path()),
            Err(Error::BadMagic { .. })
        ));
    }
}
