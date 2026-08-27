//! Rollback journal format and lock-byte page utilities (§11.13–§11.14).
//!
//! Implements the binary format for SQLite's rollback journal:
//! - Journal header (magic, page count, nonce, initial DB size, sector/page sizes)
//! - Journal page records (page number + original content + stride-200 checksum)
//! - Lock-byte page calculation (pending byte at offset `0x4000_0000`)
//!
//! The journal stores pre-images of pages modified during a transaction,
//! enabling atomic rollback on crash recovery.

use std::fmt;

use fsqlite_types::PageSize;

// ── Constants ───────────────────────────────────────────────────────────

/// Magic bytes at the start of every rollback journal file.
pub const JOURNAL_MAGIC: [u8; 8] = [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];

/// Size of the journal header (fixed portion, before sector padding).
pub const JOURNAL_HEADER_SIZE: usize = 28;

/// Checksum stride in bytes (samples every 200 bytes from the end).
pub const CHECKSUM_STRIDE: usize = 200;

/// Byte offset of the pending byte used for POSIX advisory locking.
pub const PENDING_BYTE_OFFSET: u64 = 0x4000_0000;

// ── Lock-byte page ──────────────────────────────────────────────────────

/// Compute the lock-byte page number for a given page size.
///
/// The lock-byte page is the page containing byte offset `0x4000_0000`
/// (1 GiB). This page MUST NOT be allocated for B-tree or freelist use
/// because concurrent readers may use `fcntl()` advisory locks on bytes
/// within this region.
///
/// Formula: `(0x4000_0000 / page_size) + 1`
#[must_use]
pub const fn lock_byte_page(page_size: PageSize) -> u32 {
    // PENDING_BYTE_OFFSET (0x4000_0000 = 1 GiB) fits in u32 — safe truncation.
    #[expect(clippy::cast_possible_truncation)]
    let offset_u32 = PENDING_BYTE_OFFSET as u32;
    (offset_u32 / page_size.get()) + 1
}

// ── Journal header ──────────────────────────────────────────────────────

/// Parsed rollback journal header.
///
/// Occupies 28 bytes (padded to sector boundary when written to disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalHeader {
    /// Number of page records following this header.
    /// `-1` means "compute from file size".
    pub page_count: i32,
    /// Random nonce used for the checksum calculation.
    pub nonce: u32,
    /// Size of the database (in pages) when the transaction began.
    pub initial_db_size: u32,
    /// Disk sector size in bytes.
    pub sector_size: u32,
    /// Database page size in bytes.
    pub page_size: u32,
}

/// Errors that can occur when parsing a journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// Buffer too small for the journal header.
    HeaderTooShort { needed: usize, actual: usize },
    /// Journal magic bytes do not match.
    BadMagic { actual: [u8; 8] },
    /// Buffer too small for a page record.
    RecordTooShort { needed: usize, actual: usize },
    /// Checksum mismatch on a page record.
    ChecksumMismatch {
        page_number: u32,
        expected: u32,
        actual: u32,
    },
    /// Invalid page size in journal header.
    InvalidPageSize { raw: u32 },
    /// Invalid page count in journal header.
    InvalidPageCount { raw: i32 },
}

impl fmt::Display for JournalError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderTooShort { needed, actual } => {
                write!(
                    f,
                    "journal header too short: need {needed} bytes, got {actual}"
                )
            }
            Self::BadMagic { actual } => {
                write!(f, "journal magic mismatch: got {actual:02x?}")
            }
            Self::RecordTooShort { needed, actual } => {
                write!(
                    f,
                    "journal record too short: need {needed} bytes, got {actual}"
                )
            }
            Self::ChecksumMismatch {
                page_number,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "journal checksum mismatch for page {page_number}: expected {expected:#010x}, got {actual:#010x}"
                )
            }
            Self::InvalidPageSize { raw } => {
                write!(f, "invalid page size in journal header: {raw}")
            }
            Self::InvalidPageCount { raw } => {
                write!(f, "invalid page count in journal header: {raw}")
            }
        }
    }
}

impl JournalHeader {
    /// Encode the journal header into a 28-byte buffer.
    ///
    /// The caller is responsible for padding to the sector boundary.
    #[must_use]
    pub fn encode(&self) -> [u8; JOURNAL_HEADER_SIZE] {
        let mut buf = [0u8; JOURNAL_HEADER_SIZE];
        buf[0..8].copy_from_slice(&JOURNAL_MAGIC);
        buf[8..12].copy_from_slice(&self.page_count.to_be_bytes());
        buf[12..16].copy_from_slice(&self.nonce.to_be_bytes());
        buf[16..20].copy_from_slice(&self.initial_db_size.to_be_bytes());
        buf[20..24].copy_from_slice(&self.sector_size.to_be_bytes());
        buf[24..28].copy_from_slice(&self.page_size.to_be_bytes());
        buf
    }

    /// Encode the journal header into a sector-padded buffer.
    ///
    /// The returned `Vec` has length equal to `sector_size` (minimum 28).
    #[must_use]
    pub fn encode_padded(&self) -> Vec<u8> {
        let pad_size = (self.sector_size as usize).max(JOURNAL_HEADER_SIZE);
        let mut buf = vec![0u8; pad_size];
        let header = self.encode();
        buf[..JOURNAL_HEADER_SIZE].copy_from_slice(&header);
        buf
    }

    /// Decode a journal header from a byte buffer.
    ///
    /// # Errors
    ///
    /// Returns `JournalError` if the buffer is too short or magic is wrong.
    pub fn decode(buf: &[u8]) -> Result<Self, JournalError> {
        if buf.len() < JOURNAL_HEADER_SIZE {
            return Err(JournalError::HeaderTooShort {
                needed: JOURNAL_HEADER_SIZE,
                actual: buf.len(),
            });
        }

        let mut magic = [0u8; 8];
        magic.copy_from_slice(&buf[0..8]);
        if magic != JOURNAL_MAGIC {
            return Err(JournalError::BadMagic { actual: magic });
        }

        let page_count = i32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let nonce = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let initial_db_size = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let sector_size = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        let page_size = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);

        if page_count < -1 {
            return Err(JournalError::InvalidPageCount { raw: page_count });
        }

        // Validate page size
        if PageSize::new(page_size).is_none() {
            return Err(JournalError::InvalidPageSize { raw: page_size });
        }

        Ok(Self {
            page_count,
            nonce,
            initial_db_size,
            sector_size,
            page_size,
        })
    }

    /// Compute the number of page records from the journal file size
    /// when `page_count == -1`.
    ///
    /// Formula: `(file_size - header_padded_size) / record_size`
    /// where `record_size = 4 + page_size + 4`.
    #[must_use]
    pub fn compute_page_count_from_file_size(&self, file_size: u64) -> u32 {
        let header_padded = u64::from(self.sector_size).max(JOURNAL_HEADER_SIZE as u64);
        let record_size = 4 + u64::from(self.page_size) + 4;
        if file_size <= header_padded {
            return 0;
        }
        // Journal file page count fits in u32 (SQLite max db size is ~281 TB).
        let count = (file_size - header_padded) / record_size;
        #[expect(clippy::cast_possible_truncation)]
        let result = count as u32;
        result
    }
}

// ── Journal checksum ────────────────────────────────────────────────────

/// Compute the rollback journal checksum for a page.
///
/// The checksum is `nonce + sum(data[i])` where `i` starts at
/// `page_size - CHECKSUM_STRIDE` and decrements by `CHECKSUM_STRIDE`
/// while `i > 0`. Each `data[i]` is a single byte cast to `u32`.
///
/// **Important**: `data[0]` is NEVER sampled (loop condition `i > 0`).
#[must_use]
pub fn journal_checksum(data: &[u8], nonce: u32) -> u32 {
    let page_size = data.len();
    let mut sum = nonce;
    if page_size >= CHECKSUM_STRIDE {
        let mut i = page_size - CHECKSUM_STRIDE;
        while i > 0 {
            sum = sum.wrapping_add(u32::from(data[i]));
            if i < CHECKSUM_STRIDE {
                break;
            }
            i -= CHECKSUM_STRIDE;
        }
    }
    sum
}

/// Count the number of bytes sampled by the checksum algorithm for a
/// given page size. Useful for verification.
#[must_use]
pub const fn checksum_sample_count(page_size: usize) -> usize {
    if page_size < CHECKSUM_STRIDE {
        return 0;
    }
    let mut count = 0;
    let mut i = page_size - CHECKSUM_STRIDE;
    while i > 0 {
        count += 1;
        if i < CHECKSUM_STRIDE {
            break;
        }
        i -= CHECKSUM_STRIDE;
    }
    count
}

// ── Journal page record ─────────────────────────────────────────────────

/// A single page record from a rollback journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalPageRecord {
    /// The page number (1-based) of the original page.
    pub page_number: u32,
    /// The original page content (before modification).
    pub content: Vec<u8>,
    /// The checksum of the page content.
    pub checksum: u32,
}

impl JournalPageRecord {
    /// Create a new journal page record, computing the checksum.
    #[must_use]
    pub fn new(page_number: u32, content: Vec<u8>, nonce: u32) -> Self {
        let checksum = journal_checksum(&content, nonce);
        Self {
            page_number,
            content,
            checksum,
        }
    }

    /// Size of the encoded record: 4 (pgno) + page_size + 4 (checksum).
    #[must_use]
    pub fn encoded_size(&self) -> usize {
        4 + self.content.len() + 4
    }

    /// Encode the page record into bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.encoded_size());
        buf.extend_from_slice(&self.page_number.to_be_bytes());
        buf.extend_from_slice(&self.content);
        buf.extend_from_slice(&self.checksum.to_be_bytes());
        buf
    }

    /// Decode a page record from a byte buffer.
    ///
    /// # Errors
    ///
    /// Returns `JournalError` if the buffer is too short.
    pub fn decode(buf: &[u8], page_size: u32) -> Result<Self, JournalError> {
        let needed = 4 + page_size as usize + 4;
        if buf.len() < needed {
            return Err(JournalError::RecordTooShort {
                needed,
                actual: buf.len(),
            });
        }

        let page_number = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let content = buf[4..4 + page_size as usize].to_vec();
        let cksum_offset = 4 + page_size as usize;
        let checksum = u32::from_be_bytes([
            buf[cksum_offset],
            buf[cksum_offset + 1],
            buf[cksum_offset + 2],
            buf[cksum_offset + 3],
        ]);

        Ok(Self {
            page_number,
            content,
            checksum,
        })
    }

    /// Verify the checksum of this page record against the given nonce.
    ///
    /// # Errors
    ///
    /// Returns `JournalError::ChecksumMismatch` if verification fails.
    pub fn verify_checksum(&self, nonce: u32) -> Result<(), JournalError> {
        let expected = journal_checksum(&self.content, nonce);
        if expected == self.checksum {
            Ok(())
        } else {
            Err(JournalError::ChecksumMismatch {
                page_number: self.page_number,
                expected,
                actual: self.checksum,
            })
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Page size constraint tests ──────────────────────────────────────

    #[test]
    fn test_page_size_valid_powers_of_two() {
        let valid = [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536];
        for &size in &valid {
            assert!(
                PageSize::new(size).is_some(),
                "page size {size} should be valid"
            );
        }
    }

    #[test]
    fn test_page_size_invalid_rejected() {
        let invalid = [0, 1, 256, 511, 513, 3000, 4095, 4097, 131_072];
        for &size in &invalid {
            assert!(
                PageSize::new(size).is_none(),
                "page size {size} should be rejected"
            );
        }
    }

    #[test]
    fn test_page_size_65536_encoding() {
        let ps = PageSize::new(65536).expect("65536 is valid");
        assert_eq!(ps.get(), 65536);
        // In the database header, 65536 is encoded as value 1.
        // PageSize::new(1) should be None (not a valid page size by itself).
        assert!(PageSize::new(1).is_none());
    }

    #[test]
    fn test_page_size_default_4096() {
        assert_eq!(PageSize::default().get(), 4096);
    }

    // ── Lock-byte page tests ────────────────────────────────────────────

    #[test]
    fn test_lock_byte_page_4096() {
        let ps = PageSize::new(4096).unwrap();
        assert_eq!(lock_byte_page(ps), 262_145);
    }

    #[test]
    fn test_lock_byte_page_512() {
        let ps = PageSize::new(512).unwrap();
        assert_eq!(lock_byte_page(ps), 2_097_153);
    }

    #[test]
    fn test_lock_byte_page_65536() {
        let ps = PageSize::new(65536).unwrap();
        assert_eq!(lock_byte_page(ps), 16_385);
    }

    #[test]
    fn test_lock_byte_page_1024() {
        let ps = PageSize::new(1024).unwrap();
        // (0x40000000 / 1024) + 1 = 1048576 + 1 = 1048577
        assert_eq!(lock_byte_page(ps), 1_048_577);
    }

    // ── Journal header tests ────────────────────────────────────────────

    #[test]
    fn test_journal_header_magic() {
        assert_eq!(
            JOURNAL_MAGIC,
            [0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
        );
    }

    #[test]
    fn test_journal_header_encode_decode() {
        let header = JournalHeader {
            page_count: 10,
            nonce: 0xDEAD_BEEF,
            initial_db_size: 50,
            sector_size: 512,
            page_size: 4096,
        };

        let encoded = header.encode();
        assert_eq!(encoded.len(), JOURNAL_HEADER_SIZE);

        // Magic bytes at the start
        assert_eq!(&encoded[0..8], &JOURNAL_MAGIC);

        let decoded = JournalHeader::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded, header);
    }

    #[test]
    fn test_journal_header_decode_too_short() {
        let buf = [0u8; 20];
        let err = JournalHeader::decode(&buf).unwrap_err();
        assert!(matches!(
            err,
            JournalError::HeaderTooShort {
                needed: 28,
                actual: 20
            }
        ));
    }

    #[test]
    fn test_journal_header_decode_bad_magic() {
        let mut buf = [0u8; 28];
        buf[0..8].copy_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
        // Set valid page size to avoid that error path
        buf[24..28].copy_from_slice(&4096u32.to_be_bytes());
        let err = JournalHeader::decode(&buf).unwrap_err();
        assert!(matches!(err, JournalError::BadMagic { .. }));
    }

    #[test]
    fn test_journal_header_decode_invalid_page_size() {
        let header = JournalHeader {
            page_count: 1,
            nonce: 0,
            initial_db_size: 1,
            sector_size: 512,
            page_size: 4096,
        };
        let mut encoded = header.encode();
        // Corrupt the page size to an invalid value (3000)
        encoded[24..28].copy_from_slice(&3000u32.to_be_bytes());
        let err = JournalHeader::decode(&encoded).unwrap_err();
        assert!(matches!(err, JournalError::InvalidPageSize { raw: 3000 }));
    }

    #[test]
    fn test_journal_header_rejects_invalid_negative_page_count() {
        let header = JournalHeader {
            page_count: -2,
            nonce: 0,
            initial_db_size: 1,
            sector_size: 512,
            page_size: 4096,
        };

        let err = JournalHeader::decode(&header.encode()).unwrap_err();
        assert!(matches!(err, JournalError::InvalidPageCount { raw: -2 }));
    }

    #[test]
    fn test_journal_header_sector_padding() {
        let header = JournalHeader {
            page_count: 5,
            nonce: 42,
            initial_db_size: 100,
            sector_size: 512,
            page_size: 4096,
        };

        let padded = header.encode_padded();
        assert_eq!(padded.len(), 512, "padded to sector_size");
        // First 28 bytes are the header, rest is zeros
        assert_eq!(&padded[0..8], &JOURNAL_MAGIC);
        assert!(padded[28..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_journal_header_sector_padding_small_sector() {
        let header = JournalHeader {
            page_count: 1,
            nonce: 0,
            initial_db_size: 1,
            sector_size: 16, // smaller than header
            page_size: 512,
        };

        let padded = header.encode_padded();
        // Minimum padded size is JOURNAL_HEADER_SIZE (28)
        assert_eq!(padded.len(), JOURNAL_HEADER_SIZE);
    }

    #[test]
    fn test_journal_page_count_minus_one() {
        let header = JournalHeader {
            page_count: -1,
            nonce: 0,
            initial_db_size: 10,
            sector_size: 512,
            page_size: 4096,
        };

        // record_size = 4 + 4096 + 4 = 4104
        // file_size = 512 (header) + 3 * 4104 = 512 + 12312 = 12824
        let computed = header.compute_page_count_from_file_size(12824);
        assert_eq!(computed, 3);
    }

    #[test]
    fn test_journal_page_count_from_empty() {
        let header = JournalHeader {
            page_count: -1,
            nonce: 0,
            initial_db_size: 0,
            sector_size: 512,
            page_size: 4096,
        };

        // File size smaller than header
        assert_eq!(header.compute_page_count_from_file_size(100), 0);
        // File size exactly header — no records
        assert_eq!(header.compute_page_count_from_file_size(512), 0);
    }

    // ── Journal checksum tests ──────────────────────────────────────────

    #[test]
    fn test_journal_checksum_algorithm() {
        // 4096-byte page: samples at offsets 3896, 3696, ..., 296, 96
        // i starts at 4096-200=3896, decrements by 200 while i > 0
        let mut data = vec![0u8; 4096];
        // Set known values at sample positions
        data[3896] = 1;
        data[3696] = 2;
        data[3496] = 3;
        // data[0] should NOT be sampled
        data[0] = 0xFF;

        let nonce = 100;
        let cksum = journal_checksum(&data, nonce);

        // Manually compute: nonce + data[3896] + data[3696] + data[3496] + ... + data[96]
        // Only 3 non-zero values: 1 + 2 + 3 = 6
        assert_eq!(cksum, nonce + 1 + 2 + 3);
    }

    #[test]
    fn test_journal_checksum_data0_never_sampled() {
        let mut data = vec![0u8; 4096];
        let nonce = 0;

        let cksum_a = journal_checksum(&data, nonce);

        // Change data[0] — checksum should NOT change
        data[0] = 0xFF;
        let cksum_b = journal_checksum(&data, nonce);

        assert_eq!(cksum_a, cksum_b, "data[0] must NOT be sampled");
    }

    #[test]
    fn test_journal_checksum_sample_count_4096() {
        // For 4096-byte page:
        // Samples: 3896, 3696, 3496, ..., 296, 96
        // count = (3896 - 96) / 200 + 1 = 3800/200 + 1 = 19 + 1 = 20
        assert_eq!(checksum_sample_count(4096), 20);
    }

    #[test]
    fn test_journal_checksum_sample_count_512() {
        // For 512-byte page:
        // Start: 512-200=312; samples: 312, 112
        // 312 > 0 -> count+1, 312-200=112; 112 > 0 -> count+1, 112-200 < 200 -> break
        assert_eq!(checksum_sample_count(512), 2);
    }

    #[test]
    fn test_journal_checksum_sample_count_1024() {
        // Start: 1024-200=824; samples: 824, 624, 424, 224, 24
        assert_eq!(checksum_sample_count(1024), 5);
    }

    #[test]
    fn test_journal_checksum_small_page() {
        // Page smaller than stride — no samples
        assert_eq!(checksum_sample_count(100), 0);

        let data = vec![0u8; 100];
        // Checksum is just the nonce
        assert_eq!(journal_checksum(&data, 42), 42);
    }

    // ── Journal page record tests ───────────────────────────────────────

    #[test]
    fn test_journal_page_record_encode_decode() {
        let content = vec![0xAB; 4096];
        let nonce = 0x1234_5678;
        let record = JournalPageRecord::new(3, content.clone(), nonce);

        assert_eq!(record.page_number, 3);
        assert_eq!(record.content, content);

        let encoded = record.encode();
        assert_eq!(encoded.len(), 4 + 4096 + 4);

        // First 4 bytes: page number in big-endian
        assert_eq!(&encoded[0..4], &3u32.to_be_bytes());

        let decoded = JournalPageRecord::decode(&encoded, 4096).expect("decode ok");
        assert_eq!(decoded.page_number, record.page_number);
        assert_eq!(decoded.content, record.content);
        assert_eq!(decoded.checksum, record.checksum);
    }

    #[test]
    fn test_journal_page_record_checksum_verify() {
        let content = vec![42u8; 4096];
        let nonce = 99;
        let record = JournalPageRecord::new(1, content, nonce);

        assert!(record.verify_checksum(nonce).is_ok());
    }

    #[test]
    fn test_journal_page_record_checksum_mismatch() {
        let content = vec![42u8; 4096];
        let nonce = 99;
        let record = JournalPageRecord::new(1, content, nonce);

        // Use a different nonce for verification — should fail
        let err = record.verify_checksum(nonce + 1).unwrap_err();
        assert!(matches!(
            err,
            JournalError::ChecksumMismatch { page_number: 1, .. }
        ));
    }

    #[test]
    fn test_journal_page_record_corruption_detected() {
        let content = vec![0u8; 4096];
        let nonce = 100;
        let mut record = JournalPageRecord::new(5, content, nonce);

        // Corrupt a byte at a sampled offset (stride-200: 3896, 3696, ...)
        // Offset 3896 = page_size - 200 is the first sampled byte
        record.content[3896] = 0xFF;

        // Checksum should no longer match
        let err = record.verify_checksum(nonce).unwrap_err();
        assert!(matches!(
            err,
            JournalError::ChecksumMismatch { page_number: 5, .. }
        ));
    }

    #[test]
    fn test_journal_page_record_decode_too_short() {
        let buf = [0u8; 10];
        let err = JournalPageRecord::decode(&buf, 4096).unwrap_err();
        assert!(matches!(
            err,
            JournalError::RecordTooShort {
                needed: 4104,
                actual: 10
            }
        ));
    }

    // ── Error display tests ─────────────────────────────────────────────

    #[test]
    fn test_journal_error_display() {
        let err = JournalError::HeaderTooShort {
            needed: 28,
            actual: 10,
        };
        assert!(err.to_string().contains("28"));
        assert!(err.to_string().contains("10"));

        let err = JournalError::BadMagic { actual: [0; 8] };
        assert!(err.to_string().contains("magic"));

        let err = JournalError::ChecksumMismatch {
            page_number: 7,
            expected: 100,
            actual: 200,
        };
        let s = err.to_string();
        assert!(s.contains('7'));
        assert!(s.contains("checksum"));

        let err = JournalError::InvalidPageSize { raw: 3000 };
        assert!(err.to_string().contains("3000"));
    }

    // ── Round-trip integration ──────────────────────────────────────────

    #[test]
    fn test_journal_full_roundtrip() {
        let header = JournalHeader {
            page_count: 2,
            nonce: 0xCAFE_BABE,
            initial_db_size: 100,
            sector_size: 512,
            page_size: 4096,
        };

        // Encode header
        let header_bytes = header.encode_padded();
        assert_eq!(header_bytes.len(), 512);

        // Create two page records
        let page1_content: Vec<u8> = (0u8..=255).cycle().take(4096).collect();
        let page2_content = vec![0xFF; 4096];

        let rec1 = JournalPageRecord::new(1, page1_content.clone(), header.nonce);
        let rec2 = JournalPageRecord::new(42, page2_content.clone(), header.nonce);

        let rec1_bytes = rec1.encode();
        let rec2_bytes = rec2.encode();

        // Simulate journal file
        let mut journal_file = header_bytes;
        journal_file.extend_from_slice(&rec1_bytes);
        journal_file.extend_from_slice(&rec2_bytes);

        // Decode
        let decoded_header = JournalHeader::decode(&journal_file).expect("header ok");
        assert_eq!(decoded_header, header);

        let offset1 = 512;
        let decoded_rec1 =
            JournalPageRecord::decode(&journal_file[offset1..], 4096).expect("rec1 ok");
        assert_eq!(decoded_rec1.page_number, 1);
        assert_eq!(decoded_rec1.content, page1_content);
        decoded_rec1
            .verify_checksum(header.nonce)
            .expect("rec1 checksum ok");

        let offset2 = 512 + 4104;
        let decoded_rec2 =
            JournalPageRecord::decode(&journal_file[offset2..], 4096).expect("rec2 ok");
        assert_eq!(decoded_rec2.page_number, 42);
        assert_eq!(decoded_rec2.content, page2_content);
        decoded_rec2
            .verify_checksum(header.nonce)
            .expect("rec2 checksum ok");
    }

    #[test]
    fn test_journal_error_display_invalid_page_count() {
        let err = JournalError::InvalidPageCount { raw: -5 };
        let s = err.to_string();
        assert!(s.contains("-5"));
        assert!(s.contains("page count"));
    }

    #[test]
    fn test_journal_page_record_encoded_size() {
        let record = JournalPageRecord::new(1, vec![0u8; 4096], 0);
        assert_eq!(record.encoded_size(), 4 + 4096 + 4);
        let small = JournalPageRecord::new(2, vec![0u8; 512], 0);
        assert_eq!(small.encoded_size(), 4 + 512 + 4);
        assert_eq!(record.encode().len(), record.encoded_size());
    }

    #[test]
    fn test_compute_page_count_truncated_record() {
        let header = JournalHeader {
            page_count: -1,
            nonce: 0,
            initial_db_size: 10,
            sector_size: 512,
            page_size: 4096,
        };
        let record_size = 4 + 4096 + 4;
        let full_3 = 512 + 3 * record_size as u64;
        assert_eq!(header.compute_page_count_from_file_size(full_3), 3);
        let partial = full_3 + record_size as u64 / 2;
        assert_eq!(
            header.compute_page_count_from_file_size(partial),
            3,
            "partial trailing record must not count"
        );
    }

    #[test]
    fn test_lock_byte_page_matches_pending_byte_formula() {
        for &raw in &[512u32, 1024, 4096, 65536] {
            let ps = PageSize::new(raw).unwrap();
            let expected = (PENDING_BYTE_OFFSET / u64::from(raw)) as u32 + 1;
            assert_eq!(lock_byte_page(ps), expected);
        }
    }

    #[test]
    fn test_journal_header_minus_one_page_count_roundtrip() {
        let header = JournalHeader {
            page_count: -1,
            nonce: 0xABCD,
            initial_db_size: 5,
            sector_size: 512,
            page_size: 4096,
        };
        let decoded = JournalHeader::decode(&header.encode()).expect("decode");
        assert_eq!(decoded.page_count, -1);
        assert_eq!(decoded, header);
    }
}
