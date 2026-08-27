/// Maximum length of a TEXT or BLOB in bytes.
/// Also limits the size of a row in a table or index.
pub const MAX_LENGTH: u32 = 1_000_000_000;

/// Minimum allowed value for the length limit.
pub const MIN_LENGTH: u32 = 30;

/// Maximum size of any single memory allocation.
pub const MAX_ALLOCATION_SIZE: u32 = 2_147_483_391;

/// Maximum number of columns in a table, index, or view.
/// Also the maximum number of terms in SET, result set, GROUP BY, ORDER BY, VALUES.
pub const MAX_COLUMN: u16 = 2000;

/// Maximum length of a single SQL statement in bytes.
pub const MAX_SQL_LENGTH: u32 = 1_000_000_000;

/// Maximum depth of an expression tree.
pub const MAX_EXPR_DEPTH: u32 = 1000;

/// Maximum depth of the parser stack.
pub const MAX_PARSER_DEPTH: u32 = 2500;

/// Maximum number of terms in a compound SELECT statement.
pub const MAX_COMPOUND_SELECT: u32 = 500;

/// Maximum number of opcodes in a VDBE program.
pub const MAX_VDBE_OP: u32 = 250_000_000;

/// Maximum number of arguments to an SQL function.
pub const MAX_FUNCTION_ARG: u16 = 1000;

/// Default suggested cache size (negative means limit by memory in KB).
/// -2000 means 2048000 bytes = ~2 MB.
pub const DEFAULT_CACHE_SIZE: i32 = -2000;

/// Default number of WAL frames before auto-checkpointing.
pub const DEFAULT_WAL_AUTOCHECKPOINT: u32 = 1000;

/// Maximum number of attached databases.
pub const MAX_ATTACHED: u8 = 10;

/// Maximum value of a `?nnn` wildcard parameter number.
pub const MAX_VARIABLE_NUMBER: u32 = 32766;

/// Maximum page size in bytes (must be 65536).
pub const MAX_PAGE_SIZE: u32 = 65536;

/// Default page size in bytes.
pub const DEFAULT_PAGE_SIZE: u32 = 4096;

/// Maximum default page size (used for auto-selection).
pub const MAX_DEFAULT_PAGE_SIZE: u32 = 8192;

/// Maximum number of pages in one database file.
pub const MAX_PAGE_COUNT: u32 = 0xFFFF_FFFE;

/// Maximum length in bytes of a LIKE or GLOB pattern.
pub const MAX_LIKE_PATTERN_LENGTH: u32 = 50000;

/// Maximum depth of recursion for triggers.
pub const MAX_TRIGGER_DEPTH: u32 = 1000;

/// Maximum depth of B-tree cursor stack (max tree depth).
/// A database with 2^31 pages and a branching factor of 2 would have depth ~31.
/// SQLite uses 20 as a safe maximum.
pub const BTREE_MAX_DEPTH: u8 = 20;

/// Minimum number of cells on a B-tree page.
pub const BTREE_MIN_CELLS: u32 = 2;

/// Size of the database file header in bytes.
pub const DB_HEADER_SIZE: u32 = 100;

/// Size of a B-tree page header for leaf pages.
pub const BTREE_LEAF_HEADER_SIZE: u8 = 8;

/// Size of a B-tree page header for interior pages.
pub const BTREE_INTERIOR_HEADER_SIZE: u8 = 12;

/// Size of a cell pointer (u16) in the cell pointer array.
pub const CELL_POINTER_SIZE: u8 = 2;

/// WAL magic number (big-endian checksums).
pub const WAL_MAGIC_BE: u32 = 0x377F_0682;

/// WAL magic number (little-endian checksums).
pub const WAL_MAGIC_LE: u32 = 0x377F_0683;

/// Size of a WAL frame header in bytes.
pub const WAL_FRAME_HEADER_SIZE: u32 = 24;

/// Size of the WAL file header in bytes.
pub const WAL_HEADER_SIZE: u32 = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_values_match_sqlite() {
        assert_eq!(MAX_LENGTH, 1_000_000_000);
        assert_eq!(MAX_COLUMN, 2000);
        assert_eq!(MAX_SQL_LENGTH, 1_000_000_000);
        assert_eq!(MAX_EXPR_DEPTH, 1000);
        assert_eq!(MAX_PARSER_DEPTH, 2500);
        assert_eq!(MAX_COMPOUND_SELECT, 500);
        assert_eq!(MAX_VDBE_OP, 250_000_000);
        assert_eq!(MAX_FUNCTION_ARG, 1000);
        assert_eq!(DEFAULT_CACHE_SIZE, -2000);
        assert_eq!(DEFAULT_WAL_AUTOCHECKPOINT, 1000);
        assert_eq!(MAX_ATTACHED, 10);
        assert_eq!(MAX_VARIABLE_NUMBER, 32766);
        assert_eq!(MAX_PAGE_SIZE, 65536);
        assert_eq!(DEFAULT_PAGE_SIZE, 4096);
        assert_eq!(MAX_DEFAULT_PAGE_SIZE, 8192);
        assert_eq!(MAX_PAGE_COUNT, 0xFFFF_FFFE);
        assert_eq!(MAX_LIKE_PATTERN_LENGTH, 50000);
        assert_eq!(MAX_TRIGGER_DEPTH, 1000);
    }

    #[test]
    fn default_page_size_is_power_of_two() {
        assert!(DEFAULT_PAGE_SIZE.is_power_of_two());
    }

    #[test]
    fn max_page_size_is_power_of_two() {
        assert!(MAX_PAGE_SIZE.is_power_of_two());
    }

    #[test]
    fn wal_constants() {
        assert_eq!(WAL_FRAME_HEADER_SIZE, 24);
        assert_eq!(WAL_HEADER_SIZE, 32);
    }
}
