//! Cell-Level MVCC Boundary Classification (C2, bd-l9k8e.2)
//!
//! This module defines the boundary between LOGICAL and STRUCTURAL B-tree operations
//! for the cell-level MVCC redesign. This classification is load-bearing: it determines
//! which operations use the cheap cell-level path and which stay on the existing
//! page-level MVCC path.
//!
//! # Background: Why This Matters
//!
//! The current MVCC design creates full-page version copies for every mutation.
//! A single-row INSERT that touches 50 bytes of a 4KB page creates a 4KB page copy,
//! links it into a version chain, walks that chain on every read, and GCs it later.
//!
//! The cell-level MVCC redesign splits operations into two classes:
//! 1. **LOGICAL** (cell mutations without page structural change) -> CellVisibilityLog only
//! 2. **STRUCTURAL** (splits, merges, freelist, rebalance) -> full-page versioning
//!
//! # Classification Criteria
//!
//! ## LOGICAL Operations (Cell-Level MVCC)
//!
//! These operations modify cell contents but DO NOT change the page's structural layout
//! in a way that affects other cells. They are candidates for the cheap cell-level path.
//!
//! | Operation | Condition | Implementation |
//! |-----------|-----------|----------------|
//! | `table_insert` | Page has room (`try_insert_on_leaf` returns `Ok(true)`) | cursor.rs:2602 |
//! | `index_insert` | Page has room (`try_insert_on_leaf` returns `Ok(true)`) | cursor.rs:2453 |
//! | `delete` | Page doesn't become empty (no balance needed) | cursor.rs:2613 |
//! | Cell update (same slot) | Cell size unchanged or shrinks | Not yet implemented |
//!
//! ## STRUCTURAL Operations (Page-Level MVCC)
//!
//! These operations change the page structure itself, affecting child pointers,
//! page allocation, or requiring page redistribution. They MUST use the existing
//! full-page versioning path.
//!
//! | Operation | Trigger | Implementation |
//! |-----------|---------|----------------|
//! | `balance_deeper` | Root page split (increases tree depth) | balance.rs:96 |
//! | `balance_quick` | Fast-path rightmost leaf split | balance.rs:201 |
//! | `balance_nonroot` | 3-way sibling rebalancing | balance.rs:389 |
//! | `balance_table_leaf_local_split` | Table leaf split | balance.rs:1104 |
//! | `balance_for_insert` | `try_insert_on_leaf` returns `Ok(false)` | cursor.rs:1805 |
//! | `balance_for_delete` | Leaf becomes empty (needs merge) | cursor.rs:1966 |
//! | `balance_shallower` | Decreases tree height | balance.rs:1818 |
//! | `allocate_page` | New page from freelist or file extension | freelist.rs:154 |
//! | `free_page` | Return page to freelist | freelist.rs:179 |
//! | Overflow chain creation | Cell exceeds local storage | overflow.rs:143 |
//! | Overflow chain deletion | Freeing overflow pages | cursor.rs:2341 |
//! | Interior page operations | Always structural (child pointers) | cursor.rs:2625 |
//!
//! # Edge Cases and Decisions
//!
//! ## Edge Case 1: Cell Pointer Array Shift
//!
//! **Question:** INSERT that fits in the page but changes the cell pointer array
//! offset — is this structural?
//!
//! **Decision:** NO. Cell pointer array is part of page-local bookkeeping.
//! Adding a cell to an existing page is the common case. The cell pointer shift
//! is handled transparently during page materialization at checkpoint time.
//! This remains LOGICAL.
//!
//! **Rationale:** If we made this structural, almost every INSERT would be structural,
//! defeating the purpose of the optimization. The cell pointer array is internal
//! bookkeeping, not a structural dependency visible to other transactions.
//!
//! ## Edge Case 2: UPDATE That Changes Cell Size
//!
//! **Question:** At what threshold does an UPDATE become structural?
//!
//! **Decision:** An UPDATE becomes STRUCTURAL when:
//! 1. The new cell requires overflow (creates new pages), OR
//! 2. The new cell doesn't fit in the page (exceeds usable space)
//!
//! Otherwise, an UPDATE that grows or shrinks a cell in-place remains LOGICAL.
//!
//! **Rationale:** The cell can grow/shrink within the page's available space.
//! The content area offset adjusts, but this is the same as for INSERT.
//!
//! ## Edge Case 3: DELETE That Leaves Page Underfull
//!
//! **Question:** Do we eagerly merge or lazily defer?
//!
//! **Decision:** LAZY. Mark as underfull; merge on next structural operation or checkpoint.
//!
//! **Rationale:** Eager merging after every DELETE would make most DELETEs structural.
//! SQLite itself doesn't eagerly merge — it only merges when a page becomes completely
//! empty. We follow the same policy: DELETE is LOGICAL unless `new_count == 0`.
//!
//! ## Edge Case 4: Interior (Non-Leaf) Page Operations
//!
//! **Question:** Should interior pages use cell-level MVCC?
//!
//! **Decision:** NO. Interior pages should ALWAYS use page-level MVCC.
//!
//! **Rationale:** Interior pages contain child pointers. Modifying an interior page
//! affects the entire subtree's navigation. The complexity of tracking cell-level
//! visibility for interior nodes isn't worth it — interior page modifications are
//! rare (only during splits/merges) and inherently structural.
//!
//! ## Edge Case 5: Index Pages vs Table Pages
//!
//! **Question:** Do index leaf pages use the same boundary as table leaf pages?
//!
//! **Decision:** YES. Both table and index leaf pages follow the same LOGICAL/STRUCTURAL
//! boundary. The only difference is the CellKey representation:
//! - Table pages: `CellKey::TableRow(RowId)`
//! - Index pages: `CellKey::IndexEntry { key_hash: u64, rowid: RowId }`
//!
//! **Rationale:** The page structure is identical for table and index leaf pages.
//! Only the cell format differs, which the CellKey abstraction handles.

use fsqlite_btree::{BtreePageType, has_overflow};
use fsqlite_types::PageNumber;

// ---------------------------------------------------------------------------
// Classification Types
// ---------------------------------------------------------------------------

/// Classification of a B-tree operation for MVCC purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MvccOpClass {
    /// Logical operation: cell-level MVCC (cheap path).
    /// Only modifies cell contents without structural page changes.
    Logical,
    /// Structural operation: page-level MVCC (existing path).
    /// Modifies page structure, child pointers, or page allocation.
    Structural,
}

/// The result of attempting a B-tree mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOutcome {
    /// Mutation completed as a logical operation (cell-level MVCC).
    CompletedLogical,
    /// Mutation requires structural rebalancing (triggers page-level MVCC).
    RequiresStructural,
    /// Mutation failed (e.g., constraint violation).
    Failed,
}

// ---------------------------------------------------------------------------
// Boundary Classification Predicate
// ---------------------------------------------------------------------------

/// Describes a B-tree operation for classification purposes.
#[derive(Debug, Clone)]
pub enum BtreeOp {
    /// Table leaf INSERT at the given rowid.
    TableInsert { rowid: i64, payload_size: usize },
    /// Index leaf INSERT with the given key size.
    IndexInsert { key_size: usize },
    /// Leaf DELETE at the current cursor position.
    LeafDelete { remaining_cells: u16 },
    /// Interior page modification (always structural).
    InteriorModify,
    /// Page split (always structural).
    PageSplit,
    /// Page merge (always structural).
    PageMerge,
    /// Page allocation (always structural).
    PageAllocate,
    /// Page deallocation (always structural).
    PageFree,
    /// Overflow chain operation (always structural).
    OverflowChain,
}

/// Minimal page metadata needed for classification.
#[derive(Debug, Clone)]
pub struct PageMetadata {
    /// Page number.
    pub page_no: PageNumber,
    /// Whether this is a leaf page.
    pub is_leaf: bool,
    /// Whether this is a table (intkey) page vs index (blobkey).
    pub is_table: bool,
    /// Current number of cells on the page.
    pub cell_count: u16,
    /// Current content area offset (where cells start from the bottom).
    pub content_offset: u32,
    /// Usable page size (page_size - reserved_bytes).
    pub usable_size: u32,
    /// Header offset (100 for page 1, 0 otherwise).
    pub header_offset: usize,
    /// Page header size (8 for leaf, 12 for interior).
    pub header_size: usize,
}

impl PageMetadata {
    #[must_use]
    pub const fn page_type(&self) -> BtreePageType {
        match (self.is_leaf, self.is_table) {
            (true, true) => BtreePageType::LeafTable,
            (true, false) => BtreePageType::LeafIndex,
            (false, true) => BtreePageType::InteriorTable,
            (false, false) => BtreePageType::InteriorIndex,
        }
    }

    /// Calculate available space for a new cell.
    #[must_use]
    pub fn available_space(&self) -> usize {
        // Space between end of cell pointer array and start of content area.
        // Pointer array: header_offset + header_size + (cell_count * 2)
        // We need room for one more pointer (2 bytes) plus the cell itself.
        let ptr_array_end =
            self.header_offset + self.header_size + (usize::from(self.cell_count) + 1) * 2;

        let content_start = self.content_offset as usize;

        content_start.saturating_sub(ptr_array_end)
    }

    /// Check if a cell of the given size would fit on this page.
    #[must_use]
    pub fn cell_fits(&self, cell_size: usize) -> bool {
        self.available_space() >= cell_size
    }
}

/// Determines whether a B-tree operation is LOGICAL (cell-level MVCC) or
/// STRUCTURAL (page-level MVCC).
///
/// This is the authoritative boundary predicate for the cell-level MVCC redesign.
///
/// # Arguments
///
/// * `op` - The B-tree operation being performed.
/// * `page` - Metadata about the target page (for capacity checks).
///
/// # Returns
///
/// `MvccOpClass::Logical` for operations that can use cell-level MVCC.
/// `MvccOpClass::Structural` for operations that must use page-level MVCC.
#[must_use]
pub fn classify_btree_op(op: &BtreeOp, page: &PageMetadata) -> MvccOpClass {
    match op {
        // ────────────────────────────────────────────────────────────────────
        // LOGICAL CANDIDATES (may become structural based on page state)
        // ────────────────────────────────────────────────────────────────────
        BtreeOp::TableInsert { payload_size, .. }
        | BtreeOp::IndexInsert {
            key_size: payload_size,
        } => {
            // Only leaf pages can be logical.
            if !page.is_leaf {
                return MvccOpClass::Structural;
            }

            let Ok(payload_size_u32) = u32::try_from(*payload_size) else {
                return MvccOpClass::Structural;
            };

            // Overflow chain creation is inherently structural even when the
            // page has enough free bytes for the full payload.
            if has_overflow(payload_size_u32, page.usable_size, page.page_type()) {
                return MvccOpClass::Structural;
            }

            // Estimate cell size (conservative upper bound).
            // Table: 9 (varint payload) + 9 (varint rowid) + payload + 4 (overflow ptr optional)
            // Index: 9 (varint payload) + payload + 4 (overflow ptr optional)
            let estimated_cell_size = 22_usize.saturating_add(*payload_size);

            if page.cell_fits(estimated_cell_size) {
                MvccOpClass::Logical
            } else {
                // Page full — will trigger balance_for_insert (structural).
                MvccOpClass::Structural
            }
        }

        BtreeOp::LeafDelete { remaining_cells } => {
            // Only leaf pages can be logical.
            if !page.is_leaf {
                return MvccOpClass::Structural;
            }

            // DELETE is logical unless the page becomes empty.
            // Empty page triggers balance_for_delete (structural).
            if *remaining_cells == 0 && page.cell_count > 0 {
                // This DELETE will empty the page.
                // Note: We check cell_count > 0 because deleting from an already-empty
                // page is an error condition, not a structural operation.
                MvccOpClass::Structural
            } else {
                MvccOpClass::Logical
            }
        }

        // ────────────────────────────────────────────────────────────────────
        // ALWAYS STRUCTURAL
        // ────────────────────────────────────────────────────────────────────
        BtreeOp::InteriorModify => {
            // Interior pages always use page-level MVCC.
            MvccOpClass::Structural
        }

        BtreeOp::PageSplit
        | BtreeOp::PageMerge
        | BtreeOp::PageAllocate
        | BtreeOp::PageFree
        | BtreeOp::OverflowChain => {
            // These are inherently structural operations.
            MvccOpClass::Structural
        }
    }
}

// ---------------------------------------------------------------------------
// Function Classification Table
// ---------------------------------------------------------------------------

/// Complete classification table for all B-tree mutation functions.
///
/// This table documents every function in the fsqlite-btree crate that performs
/// mutations, along with its classification and the conditions under which
/// it may transition from LOGICAL to STRUCTURAL.
///
/// # Cursor Operations (cursor.rs)
///
/// | Function | Default Class | Becomes Structural When |
/// |----------|---------------|-------------------------|
/// | `table_insert` | Logical | Page full → `balance_for_insert` |
/// | `table_insert_from_current_position` | Logical | Page full → `balance_for_insert` |
/// | `table_insert_prechecked_absent` | Logical | Page full → `balance_for_insert` |
/// | `index_insert` | Logical | Page full → `balance_for_insert` |
/// | `index_insert_unique` | Logical | Page full → `balance_for_insert` |
/// | `delete` | Logical | Page empties → `balance_for_delete` |
/// | `replace_interior_cell` | Structural | Always (interior page) |
/// | `remove_cell_from_leaf` | Logical* | *Cell removal is logical; emptying triggers structural |
/// | `free_overflow_chain` | Structural | Always (page deallocation) |
/// | `try_insert_on_leaf` | Logical | Returns `Ok(false)` → structural path |
///
/// # Balance Operations (balance.rs)
///
/// | Function | Class | Description |
/// |----------|-------|-------------|
/// | `balance_deeper` | Structural | Root split, increases tree depth |
/// | `balance_quick` | Structural | Fast-path rightmost leaf split |
/// | `balance_nonroot` | Structural | 3-way sibling rebalancing |
/// | `balance_table_leaf_local_split` | Structural | Table leaf split |
/// | `balance_shallower` | Structural | Decreases tree depth |
/// | `split_overflowing_root` | Structural | Root page overflow split |
/// | `split_overflowing_nonroot_interior_page` | Structural | Interior page split |
/// | `insert_cell_into_page` | Mixed | Logical if fits, else structural |
///
/// # Freelist Operations (freelist.rs)
///
/// | Function | Class | Description |
/// |----------|-------|-------------|
/// | `Freelist::allocate` | Structural | Page allocation |
/// | `Freelist::deallocate` | Structural | Page deallocation |
///
/// # Overflow Operations (overflow.rs)
///
/// | Function | Class | Description |
/// |----------|-------|-------------|
/// | `write_overflow_chain` | Structural | Creates new overflow pages |
/// | `free_overflow_chain` | Structural | Frees overflow pages |
///
/// # Page Writer Operations (cursor.rs, PageWriter trait)
///
/// | Function | Class | Description |
/// |----------|-------|-------------|
/// | `write_page` | Mixed | Logical if cell update, structural if layout change |
/// | `allocate_page` | Structural | Always |
/// | `free_page` | Structural | Always |
pub const CLASSIFICATION_TABLE_VERSION: &str = "1.0.0";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_table_page(cell_count: u16, available_space: u32) -> PageMetadata {
        PageMetadata {
            page_no: PageNumber::new(2).unwrap(),
            is_leaf: true,
            is_table: true,
            cell_count,
            // Set content_offset to give us the desired available space.
            // available = content_offset - (header_offset + header_size + (cell_count + 1) * 2)
            // For page 2: header_offset = 0, header_size = 8 (leaf)
            // content_offset = available + 8 + (cell_count + 1) * 2
            content_offset: available_space + 8 + (u32::from(cell_count) + 1) * 2,
            usable_size: 4096,
            header_offset: 0,
            header_size: 8,
        }
    }

    fn interior_table_page(cell_count: u16) -> PageMetadata {
        PageMetadata {
            page_no: PageNumber::new(2).unwrap(),
            is_leaf: false,
            is_table: true,
            cell_count,
            content_offset: 4000,
            usable_size: 4096,
            header_offset: 0,
            header_size: 12, // Interior pages have 4 extra bytes for right_child
        }
    }

    #[test]
    fn test_table_insert_fits_is_logical() {
        let page = leaf_table_page(10, 1000); // 1000 bytes available
        let op = BtreeOp::TableInsert {
            rowid: 100,
            payload_size: 50,
        };

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Logical);
    }

    #[test]
    fn test_table_insert_no_room_is_structural() {
        let page = leaf_table_page(100, 30); // Only 30 bytes available
        let op = BtreeOp::TableInsert {
            rowid: 100,
            payload_size: 50, // Needs ~72 bytes (22 overhead + 50 payload)
        };

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Structural);
    }

    #[test]
    fn test_delete_with_remaining_is_logical() {
        let page = leaf_table_page(5, 500);
        let op = BtreeOp::LeafDelete { remaining_cells: 4 };

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Logical);
    }

    #[test]
    fn test_delete_emptying_page_is_structural() {
        let page = leaf_table_page(1, 500); // One cell
        let op = BtreeOp::LeafDelete { remaining_cells: 0 }; // Will empty page

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Structural);
    }

    #[test]
    fn test_interior_page_always_structural() {
        let page = interior_table_page(5);

        // Even a "small" insert to an interior page is structural.
        let op = BtreeOp::TableInsert {
            rowid: 100,
            payload_size: 10,
        };
        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Structural);
    }

    #[test]
    fn test_page_operations_always_structural() {
        let page = leaf_table_page(10, 1000);

        assert_eq!(
            classify_btree_op(&BtreeOp::PageSplit, &page),
            MvccOpClass::Structural
        );
        assert_eq!(
            classify_btree_op(&BtreeOp::PageMerge, &page),
            MvccOpClass::Structural
        );
        assert_eq!(
            classify_btree_op(&BtreeOp::PageAllocate, &page),
            MvccOpClass::Structural
        );
        assert_eq!(
            classify_btree_op(&BtreeOp::PageFree, &page),
            MvccOpClass::Structural
        );
        assert_eq!(
            classify_btree_op(&BtreeOp::OverflowChain, &page),
            MvccOpClass::Structural
        );
    }

    #[test]
    fn test_index_insert_classification() {
        let mut page = leaf_table_page(10, 1000);
        page.is_table = false; // Make it an index page

        // Small key fits
        let op = BtreeOp::IndexInsert { key_size: 50 };
        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Logical);

        // Large key doesn't fit
        let mut page_full = leaf_table_page(100, 30);
        page_full.is_table = false;
        let op_big = BtreeOp::IndexInsert { key_size: 100 };
        assert_eq!(
            classify_btree_op(&op_big, &page_full),
            MvccOpClass::Structural
        );
    }

    #[test]
    fn test_table_insert_requiring_overflow_is_structural_even_when_empty_page_has_space() {
        let page = leaf_table_page(0, 4086);
        let op = BtreeOp::TableInsert {
            rowid: 100,
            payload_size: 4062,
        };

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Structural);
    }

    #[test]
    fn test_index_insert_requiring_overflow_is_structural_even_when_page_has_space() {
        let mut page = leaf_table_page(0, 4086);
        page.is_table = false;

        let op = BtreeOp::IndexInsert { key_size: 1100 };

        assert_eq!(classify_btree_op(&op, &page), MvccOpClass::Structural);
    }
}
