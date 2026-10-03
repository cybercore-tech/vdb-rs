//! `.vectors` file: per-collection, contiguous, 32-byte-aligned `f32`
//! arrays. Layout: a 32-byte checksummed header, then 32-byte-aligned
//! vector blocks containing little-endian f32s and an eight-byte checksum.
//! See `docs/storage_layout.md`.
//!
//! Reads go through `VectorFile`, which mmaps the file. Writes go through
//! `VectorFileWriter`, which uses plain buffered I/O — mmap is for
//! zero-copy reads, not for growing a file, so writing and reading are
//! deliberately two different paths (open a `VectorFile` to read back what
//! a `VectorFileWriter` wrote, once it's flushed).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use memmap2::Mmap;

use crate::error::{Error, Result};

/// Magic bytes identifying a `.vectors` file.
pub const MAGIC: &[u8; 4] = b"VDB2";

/// Every offset (header end, and each vector block) is a multiple of this.
const ALIGN: usize = 32;

/// Header: magic(4) + dim(4) + count(8) + reserved(16) = 32, itself aligned.
const HEADER_LEN: usize = 32;

fn aligned(len: usize) -> usize {
    len.div_ceil(ALIGN) * ALIGN
}

pub(crate) fn block_len(dim: u32) -> usize {
    aligned(dim as usize * 4 + 8)
}

fn validate_header(header: &[u8]) -> Result<()> {
    if crate::checksum::checksum(&header[..16])
        != u64::from_le_bytes(header[16..24].try_into().unwrap())
        || header[24..32] != [0; 8]
    {
        return Err(Error::Corrupt(
            "vector header checksum/reserved fields".into(),
        ));
    }
    Ok(())
}

/// Validate supported dimensionality (1 through 65,536).
pub(crate) fn validate_dimension(dim: u32) -> Result<()> {
    if dim == 0 || dim > 65_536 {
        return Err(Error::InvalidInput(
            "dimension must be between 1 and 65536".into(),
        ));
    }
    Ok(())
}

fn validate_layout(dim: u32, count: u64, len: u64) -> Result<()> {
    validate_dimension(dim).map_err(|_| Error::Corrupt("invalid vector dimension".into()))?;
    let expected = count
        .checked_mul(block_len(dim) as u64)
        .and_then(|n| n.checked_add(HEADER_LEN as u64))
        .ok_or_else(|| Error::Corrupt("vector file size overflow".into()))?;
    if expected != len {
        return Err(Error::Corrupt("vector count/file length mismatch".into()));
    }
    Ok(())
}

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
        validate_header(&mmap[..HEADER_LEN])?;
        let dim = u32::from_le_bytes(mmap[4..8].try_into().unwrap());
        validate_layout(
            dim,
            u64::from_le_bytes(mmap[8..16].try_into().unwrap()),
            mmap.len() as u64,
        )?;

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

    /// Read the vector stored at `offset` (as returned by
    /// [`VectorFileWriter::append`] / recorded in a `VectorMeta`).
    pub fn read_at(&self, offset: u64) -> Result<&[f32]> {
        let offset =
            usize::try_from(offset).map_err(|_| Error::Corrupt("vector offset overflow".into()))?;
        if offset < HEADER_LEN || !(offset - HEADER_LEN).is_multiple_of(block_len(self.dim)) {
            return Err(Error::Corrupt("invalid vector block offset".into()));
        }
        let byte_len = self.dim as usize * 4;
        let end = offset
            .checked_add(byte_len)
            .ok_or_else(|| Error::Corrupt("vector range overflow".into()))?;
        let bytes = self
            .mmap
            .get(offset..end)
            .ok_or_else(|| Error::Corrupt("vector outside file".into()))?;
        if cfg!(target_endian = "big") {
            return Err(Error::Corrupt(
                "zero-copy vectors require a little-endian host".into(),
            ));
        }

        let checksum_end = end
            .checked_add(8)
            .ok_or_else(|| Error::Corrupt("checksum range overflow".into()))?;
        let stored = self
            .mmap
            .get(end..checksum_end)
            .ok_or_else(|| Error::Corrupt("missing vector checksum".into()))?;
        if crate::checksum::checksum(bytes) != u64::from_le_bytes(stored.try_into().unwrap()) {
            return Err(Error::Corrupt("vector payload checksum".into()));
        }
        // Safety: `bytes` is exactly `dim` little-endian f32s written by
        // `VectorFileWriter::append`, and is 4-byte aligned because every
        // vector block starts at a 32-byte-aligned offset.
        Ok(bytemuck_cast_f32(bytes))
    }

    /// The raw mmap'd bytes (header + data).
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }
}

/// Reinterpret a byte slice of native-endian `f32`s without an extra copy.
///
/// # Panics
///
/// Panics if `bytes.len()` isn't a multiple of 4 or `bytes` isn't 4-byte
/// aligned — both are guaranteed by `VectorFileWriter`'s layout.
fn bytemuck_cast_f32(bytes: &[u8]) -> &[f32] {
    assert_eq!(
        bytes.len() % 4,
        0,
        "vector byte slice must be a multiple of 4 bytes"
    );
    assert_eq!(
        bytes.as_ptr().align_offset(4),
        0,
        "vector byte slice must be 4-byte aligned"
    );
    // Safety: length and alignment checked above; f32 has no invalid bit patterns.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), bytes.len() / 4) }
}

/// A sequential writer for a `.vectors` file. Uses plain buffered I/O, not
/// mmap — see the module docs for why.
pub struct VectorFileWriter {
    file: BufWriter<File>,
    dim: u32,
    count: u64,
}

impl VectorFileWriter {
    /// Create a new `.vectors` file at `path` and write its header.
    /// Fails if `path` already exists.
    pub fn create(path: &Path, dim: u32) -> Result<Self> {
        validate_dimension(dim)?;
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let mut file = BufWriter::new(file);
        file.write_all(MAGIC)?;
        file.write_all(&dim.to_le_bytes())?;
        file.write_all(&0u64.to_le_bytes())?; // count, patched by flush()
        let mut prefix = Vec::from(MAGIC.as_slice());
        prefix.extend(dim.to_le_bytes());
        prefix.extend(0u64.to_le_bytes());
        file.write_all(&crate::checksum::checksum(&prefix).to_le_bytes())?;
        file.write_all(&[0u8; 8])?; // reserved
        Ok(Self {
            file,
            dim,
            count: 0,
        })
    }

    /// Byte offset where the next append will land.
    pub fn next_offset(&self) -> u64 {
        HEADER_LEN as u64 + self.count * block_len(self.dim) as u64
    }

    /// Append one vector. Returns the byte offset it was written at, for
    /// storing in a `VectorMeta.offset`.
    pub fn append(&mut self, vector: &[f32]) -> Result<u64> {
        if vector.iter().any(|v| !v.is_finite()) {
            return Err(Error::InvalidInput(
                "vectors must contain finite values".into(),
            ));
        }
        if vector.len() as u32 != self.dim {
            return Err(Error::DimensionMismatch {
                expected: self.dim,
                got: vector.len() as u32,
            });
        }

        let block_len = block_len(self.dim);
        let offset = HEADER_LEN as u64 + self.count * block_len as u64;

        let bytes: Vec<u8> = vector.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.file.write_all(&bytes)?;
        self.file
            .write_all(&crate::checksum::checksum(&bytes).to_le_bytes())?;
        self.file
            .write_all(&vec![0u8; block_len - bytes.len() - 8])?;

        self.count += 1;
        Ok(offset)
    }

    /// Open an existing `.vectors` file for appending more vectors.
    /// Reads the current header to recover `dim`/`count` and seeks to the
    /// end so the next [`VectorFileWriter::append`] lands after the
    /// existing data.
    pub fn open_append(path: &Path) -> Result<Self> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        let mut header = [0u8; HEADER_LEN];
        file.seek(SeekFrom::Start(0))?;
        std::io::Read::read_exact(&mut file, &mut header)?;
        if &header[0..4] != MAGIC {
            return Err(Error::BadMagic {
                expected: MAGIC,
                got: header[0..4].to_vec(),
            });
        }
        validate_header(&header)?;
        let dim = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let count = u64::from_le_bytes(header[8..16].try_into().unwrap());

        validate_layout(dim, count, file.metadata()?.len())?;
        file.seek(SeekFrom::End(0))?;

        Ok(Self {
            file: BufWriter::new(file),
            dim,
            count,
        })
    }

    /// Flush buffered writes and patch the header's `count` field.
    pub fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        let file = self.file.get_mut();
        let pos = file.stream_position()?;
        file.seek(SeekFrom::Start(8))?;
        file.write_all(&self.count.to_le_bytes())?;
        let mut prefix = Vec::from(MAGIC.as_slice());
        prefix.extend(self.dim.to_le_bytes());
        prefix.extend(self.count.to_le_bytes());
        file.write_all(&crate::checksum::checksum(&prefix).to_le_bytes())?;
        file.seek(SeekFrom::Start(pos))?;
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_append_continues_writing_after_a_prior_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docs.vectors");

        let mut writer = VectorFileWriter::create(&path, 3).unwrap();
        let off_a = writer.append(&[1.0, 2.0, 3.0]).unwrap();
        writer.flush().unwrap();
        drop(writer);

        let mut writer = VectorFileWriter::open_append(&path).unwrap();
        let off_b = writer.append(&[4.0, 5.0, 6.0]).unwrap();
        writer.flush().unwrap();

        let vf = VectorFile::open(&path).unwrap();
        assert_eq!(vf.count(), 2);
        assert_eq!(vf.read_at(off_a).unwrap(), &[1.0, 2.0, 3.0]);
        assert_eq!(vf.read_at(off_b).unwrap(), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn write_then_read_round_trips_multiple_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docs.vectors");

        let mut writer = VectorFileWriter::create(&path, 3).unwrap();
        let off_a = writer.append(&[1.0, 2.0, 3.0]).unwrap();
        let off_b = writer.append(&[4.0, 5.0, 6.0]).unwrap();
        writer.flush().unwrap();

        let vf = VectorFile::open(&path).unwrap();
        assert_eq!(vf.dim(), 3);
        assert_eq!(vf.count(), 2);
        assert_eq!(vf.read_at(off_a).unwrap(), &[1.0, 2.0, 3.0]);
        assert_eq!(vf.read_at(off_b).unwrap(), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn append_offsets_are_32_byte_aligned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docs.vectors");
        // dim=3 -> 12 raw bytes/vector, padded to 32.
        let mut writer = VectorFileWriter::create(&path, 3).unwrap();

        let off_a = writer.append(&[0.0, 0.0, 0.0]).unwrap();
        let off_b = writer.append(&[1.0, 1.0, 1.0]).unwrap();

        assert_eq!(off_a % ALIGN as u64, 0);
        assert_eq!(off_b % ALIGN as u64, 0);
        assert_eq!(off_b - off_a, ALIGN as u64);
    }

    #[test]
    fn append_rejects_a_vector_of_the_wrong_dimension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docs.vectors");
        let mut writer = VectorFileWriter::create(&path, 3).unwrap();

        let err = writer.append(&[1.0, 2.0]).unwrap_err();

        assert!(matches!(
            err,
            Error::DimensionMismatch {
                expected: 3,
                got: 2
            }
        ));
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
