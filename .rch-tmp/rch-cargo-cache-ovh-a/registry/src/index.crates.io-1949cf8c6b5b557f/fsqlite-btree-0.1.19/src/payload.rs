//! Payload reading abstraction (§11, bd-2kvo).
//!
//! A B-tree cell's payload may be stored entirely on the page ("local") or
//! may spill into an overflow chain. This module provides [`read_payload`],
//! a unified entry-point that reassembles the complete payload regardless
//! of where the bytes live.

use crate::cell::{self, BtreePageType, CellRef};
use crate::instrumentation;
use crate::overflow;
use fsqlite_error::{FrankenError, Result};
use fsqlite_types::PageNumber;
use tracing::debug;

/// Read the complete payload for a cell, resolving overflow if necessary.
///
/// `cell` is the parsed cell reference from [`CellRef::parse`].
/// `page` is the raw page data containing the cell.
/// `usable_size` is the usable page size (page_size - reserved_bytes).
/// `read_page` is a callback to read any page by number (for overflow).
///
/// For cells without overflow, this is a simple copy of the local payload.
/// For cells with overflow, it calls into the overflow chain reader.
pub fn read_payload<F>(
    cell: &CellRef,
    page: &[u8],
    usable_size: u32,
    read_page: &mut F,
) -> Result<Vec<u8>>
where
    F: FnMut(PageNumber) -> Result<Vec<u8>>,
{
    let local = cell.local_payload(page);

    if let Some(first_overflow) = cell.overflow_page {
        overflow::read_overflow_chain(
            local,
            first_overflow,
            cell.payload_size,
            usable_size,
            read_page,
        )
    } else {
        instrumentation::record_owned_payload_materialization(local.len());
        Ok(local.to_vec())
    }
}

/// Compute the total on-page size of a cell.
///
/// Accounts for the left-child pointer, varints, local payload, and
/// overflow pointer. `cell_start` is the byte offset where the cell
/// begins on the page.
#[must_use]
pub fn cell_on_page_size(cell: &CellRef, cell_start: usize) -> usize {
    let mut size = cell.payload_offset - cell_start + cell.local_size as usize;
    if cell.overflow_page.is_some() {
        size += 4;
    }
    size
}

/// Write a cell's payload, splitting between local storage and overflow
/// chain as needed.
///
/// Returns `(local_data, overflow_page)` where `overflow_page` is `None`
/// if the payload fits entirely in local storage.
///
/// `payload` is the complete payload bytes.
/// `page_type` is the type of B-tree page the cell will be written to.
/// `usable_size` is the usable page size.
/// `allocate_page` allocates a new page.
/// `write_page` writes raw data to a page.
pub fn write_payload<A, W>(
    payload: &[u8],
    page_type: BtreePageType,
    usable_size: u32,
    full_page_size: u32,
    allocate_page: &mut A,
    write_page: &mut W,
) -> Result<(Vec<u8>, Option<PageNumber>)>
where
    A: FnMut() -> Result<PageNumber>,
    W: FnMut(PageNumber, &[u8]) -> Result<()>,
{
    let payload_size = u32::try_from(payload.len()).map_err(|_| FrankenError::TooBig)?;
    let local_size = cell::local_payload_size(payload_size, usable_size, page_type) as usize;

    if local_size >= payload.len() {
        // Entire payload fits locally.
        debug!(
            cell_type = ?page_type,
            payload_len = payload_size,
            overflow = false,
            "encoded btree cell boundary"
        );
        return Ok((payload.to_vec(), None));
    }

    let local_data = payload[..local_size].to_vec();
    let overflow_data = &payload[local_size..];

    let first_overflow = overflow::write_overflow_chain(
        overflow_data,
        usable_size,
        full_page_size,
        allocate_page,
        write_page,
    )?;

    debug!(
        cell_type = ?page_type,
        payload_len = payload_size,
        overflow = true,
        "encoded btree cell boundary"
    );

    Ok((local_data, Some(first_overflow)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use fsqlite_error::FrankenError;
    use std::collections::HashMap;

    /// Build a minimal leaf table page with a single cell containing the
    /// given payload, returning `(page, cell_offset, usable_size)`.
    fn build_leaf_table_page(payload: &[u8], usable_size: u32) -> (Vec<u8>, usize) {
        let page_size = usable_size as usize;
        let mut page = vec![0u8; page_size];

        // Write a leaf table page header.
        page[0] = 0x0D; // LeafTable
        page[3..5].copy_from_slice(&1u16.to_be_bytes()); // 1 cell

        // Place cell at a reasonable offset.
        let cell_offset = page_size / 2;

        // Cell content: [payload_size varint] [rowid varint] [payload]
        let mut pos = cell_offset;

        // payload_size varint (simple: fits in 1-2 bytes for our tests).
        let ps = payload.len();
        if ps < 128 {
            page[pos] = ps as u8;
            pos += 1;
        } else {
            page[pos] = 0x80 | ((ps >> 7) as u8 & 0x7F);
            page[pos + 1] = (ps & 0x7F) as u8;
            pos += 2;
        }

        // rowid = 1
        page[pos] = 1;
        pos += 1;

        // Local payload (may be truncated if overflow).
        let local =
            cell::local_payload_size(ps as u32, usable_size, BtreePageType::LeafTable) as usize;
        let local_bytes = local.min(payload.len());
        let end = pos + local_bytes;
        if end <= page.len() {
            page[pos..end].copy_from_slice(&payload[..local_bytes]);
        }

        // If overflow, write overflow page pointer after local payload.
        if local_bytes < payload.len() {
            let ptr_offset = pos + local_bytes;
            if ptr_offset + 4 <= page.len() {
                // Overflow page = 100 (we'll set up the overflow pages externally).
                page[ptr_offset..ptr_offset + 4].copy_from_slice(&100u32.to_be_bytes());
            }
        }

        (page, cell_offset)
    }

    #[test]
    fn test_read_payload_local_only() {
        let payload = b"hello world";
        let usable_size = 4096u32;
        let (page, cell_offset) = build_leaf_table_page(payload, usable_size);

        let cell =
            CellRef::parse(&page, cell_offset, BtreePageType::LeafTable, usable_size).unwrap();

        assert!(cell.overflow_page.is_none());

        let result = read_payload(&cell, &page, usable_size, &mut |_| {
            Err(FrankenError::internal("should not read overflow"))
        })
        .unwrap();

        assert_eq!(result, payload);
    }

    #[test]
    fn test_write_payload_local_only() {
        let payload = b"short payload";
        let usable_size = 4096u32;

        let (local, overflow) = write_payload(
            payload,
            BtreePageType::LeafTable,
            usable_size,
            usable_size, // full_page_size == usable_size when reserved_bytes == 0
            &mut || Err(FrankenError::internal("should not allocate")),
            &mut |_, _| Err(FrankenError::internal("should not write overflow")),
        )
        .unwrap();

        assert_eq!(local, payload);
        assert!(overflow.is_none());
    }

    #[test]
    fn test_write_payload_with_overflow() {
        // SQLite minimum usable_size is 480; use 512 to stay realistic.
        let usable_size = 512u32;
        // max_local for leaf table = 512 - 35 = 477.
        let payload: Vec<u8> = (0u8..=255).cycle().take(1000).collect();

        let mut pages: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut next_page = 50u32;

        let (local, overflow) = write_payload(
            &payload,
            BtreePageType::LeafTable,
            usable_size,
            usable_size,
            &mut || {
                let pgno = PageNumber::new(next_page).unwrap();
                next_page += 1;
                Ok(pgno)
            },
            &mut |pgno, data| {
                pages.insert(pgno.get(), data.to_vec());
                Ok(())
            },
        )
        .unwrap();

        assert!(overflow.is_some());
        assert!(local.len() < payload.len());
        assert_eq!(&payload[..local.len()], &local);

        // Verify the overflow chain can be read back.
        let result = overflow::read_overflow_chain(
            &local,
            overflow.unwrap(),
            payload.len() as u32,
            usable_size,
            &mut |pgno| {
                pages
                    .get(&pgno.get())
                    .cloned()
                    .ok_or_else(|| FrankenError::internal("page not found"))
            },
        )
        .unwrap();

        assert_eq!(result, payload);
    }

    #[test]
    fn test_write_read_payload_roundtrip() {
        // Use the minimum valid usable_size (480) to exercise overflow.
        // max_local for leaf table = 480 - 35 = 445.
        let usable_size = 480u32;
        let payload: Vec<u8> = (0u8..=255).cycle().take(2000).collect();

        let mut pages: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut next_page = 10u32;

        let (local, overflow_pgno) = write_payload(
            &payload,
            BtreePageType::LeafTable,
            usable_size,
            usable_size,
            &mut || {
                let pgno = PageNumber::new(next_page).unwrap();
                next_page += 1;
                Ok(pgno)
            },
            &mut |pgno, data| {
                pages.insert(pgno.get(), data.to_vec());
                Ok(())
            },
        )
        .unwrap();

        // Build a fake CellRef to test read_payload.
        let cell = CellRef {
            left_child: None,
            rowid: Some(1),
            payload_size: payload.len() as u32,
            local_size: local.len() as u32,
            payload_offset: 0, // We'll use a custom page.
            overflow_page: overflow_pgno,
        };

        // Build a page that starts with the local payload at offset 0.
        let mut page = vec![0u8; usable_size as usize];
        page[..local.len()].copy_from_slice(&local);

        let result = read_payload(&cell, &page, usable_size, &mut |pgno| {
            pages
                .get(&pgno.get())
                .cloned()
                .ok_or_else(|| FrankenError::internal("page not found"))
        })
        .unwrap();

        assert_eq!(result, payload);
    }

    #[test]
    fn test_cell_on_page_size_no_overflow() {
        let cell = CellRef {
            left_child: None,
            rowid: Some(1),
            payload_size: 50,
            local_size: 50,
            payload_offset: 10, // varint headers took 10 bytes from cell_start=0.
            overflow_page: None,
        };
        // 10 bytes header + 50 bytes payload = 60.
        assert_eq!(cell_on_page_size(&cell, 0), 60);
    }

    #[test]
    fn test_cell_on_page_size_with_overflow() {
        let cell = CellRef {
            left_child: None,
            rowid: Some(1),
            payload_size: 5000,
            local_size: 100,
            payload_offset: 5,
            overflow_page: Some(PageNumber::new(42).unwrap()),
        };
        // 5 bytes header + 100 bytes local + 4 bytes overflow ptr = 109.
        assert_eq!(cell_on_page_size(&cell, 0), 109);
    }

    #[test]
    fn test_cell_on_page_size_interior() {
        let cell = CellRef {
            left_child: Some(PageNumber::new(7).unwrap()),
            rowid: None,
            payload_size: 20,
            local_size: 20,
            payload_offset: 6, // 4 left_child + 2 varint.
            overflow_page: None,
        };
        // 6 bytes header + 20 bytes payload = 26.
        assert_eq!(cell_on_page_size(&cell, 0), 26);
    }

    #[test]
    fn test_write_payload_index_page() {
        let usable_size = 512u32;
        // max_local for index = (512-12)*64/255 - 23 = (500*64)/255 - 23 = 32000/255 - 23 = 125 - 23 = 102
        // min_local = (512-12)*32/255 - 23 = 16000/255 - 23 = 62 - 23 = 39
        let payload: Vec<u8> = (0u8..200).collect();

        let mut pages: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut next_page = 20u32;

        let (local, overflow) = write_payload(
            &payload,
            BtreePageType::LeafIndex,
            usable_size,
            usable_size,
            &mut || {
                let pgno = PageNumber::new(next_page).unwrap();
                next_page += 1;
                Ok(pgno)
            },
            &mut |pgno, data| {
                pages.insert(pgno.get(), data.to_vec());
                Ok(())
            },
        )
        .unwrap();

        assert!(overflow.is_some());
        assert!(local.len() <= 102);
        assert!(local.len() >= 39);

        // Read back.
        let result = overflow::read_overflow_chain(
            &local,
            overflow.unwrap(),
            payload.len() as u32,
            usable_size,
            &mut |pgno| {
                pages
                    .get(&pgno.get())
                    .cloned()
                    .ok_or_else(|| FrankenError::internal("page not found"))
            },
        )
        .unwrap();

        assert_eq!(result, payload);
    }
}
