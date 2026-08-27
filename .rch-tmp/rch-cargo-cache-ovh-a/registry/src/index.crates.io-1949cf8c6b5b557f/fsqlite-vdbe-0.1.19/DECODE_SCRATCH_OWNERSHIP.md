# Decode & Materialization Buffer Ownership Model

> **Bead:** bd-db300.4.3.1 (D3.1)
> **Date:** 2026-03-23
> **Author:** IronVault (claude-opus-4-6)
> **Decision:** Per-cursor scratch buffers with explicit lifetime and reset rules.

## Decision

**Scratch buffers, NOT arena allocator.**

The decode and materialization hot paths use **cursor-local scratch buffers** with explicit
invalidation and capacity retention. This is the correct model for FrankenSQLite because:

1. Allocation lifetimes are short and well-defined (per-row or per-statement)
2. No cross-cursor or cross-statement data retention is needed
3. Cursor-local ownership eliminates contention in concurrent-writer mode
4. Bulk deallocation happens implicitly (cursor destruction, statement reset)
5. An arena would add per-allocation metadata overhead for tiny, uniform allocations

## Buffer Inventory

| Buffer | Owner | Lifetime | Reset Rule | Typical Size |
|--------|-------|----------|------------|--------------|
| `payload_buf: Vec<u8>` | StorageCursor | Cursor | Cleared on `payload_into()` | 100B–1KB |
| `row_decode: RecordDecodeScratch` | StorageCursor | Cursor | Invalidated on position change (stamp mismatch) | 200B base |
| `target_vals_buf: Vec<SqliteValue>` | StorageCursor | Cursor | Cleared before each index op | 8×24B |
| `cur_vals_buf: Vec<SqliteValue>` | StorageCursor | Cursor | Cleared before each index op | 8×24B |
| `make_record_buf: Vec<u8>` | VdbeEngine | Statement | Swap-take pattern on every MakeRecord | 100B–10KB |
| `results: Vec<SmallVec<[SqliteValue; 16]>>` | VdbeEngine | Statement | `take_results()` swaps ownership | Pre-alloc 64 rows |
| `registers: Vec<Register>` | VdbeEngine | Statement | Resize/clear on `reset_for_reuse` | Varies |
| Overflow chain buffer | Caller-provided | Operation | Passed from B-tree layer | Up to page size |

## Lifetime Rules

### Rule 1: Cursor buffers are cursor-scoped
- `payload_buf`, `row_decode`, `target_vals_buf`, `cur_vals_buf` live as long as the `StorageCursor`.
- They are **not shared** between cursors. Each cursor owns its own set.
- Capacity is retained across position changes; only content is invalidated.

### Rule 2: Statement buffers are statement-scoped
- `make_record_buf`, `results`, and the register file live as long as the `VdbeEngine`.
- On `reset_for_reuse()`, content is cleared but capacity is retained.
- On `take_results()`, the results Vec is swapped out (zero-copy handoff to caller).

### Rule 3: Position-change invalidation via stamp
- `RecordDecodeScratch` tracks a `position_stamp: (u32, u16)` from the B-tree layer.
- On cursor movement, the stamp changes. The decode cache is invalidated (values cleared,
  decoded_mask reset), but the `header_offsets` and `values` Vecs retain capacity.
- For ≤64 columns: lazy per-column decode with bitmap tracking.
- For >64 columns: full eager decode (decoded_mask = u64::MAX).

### Rule 4: No cross-transaction sharing
- Transaction commit/rollback destroys cursors, which destroys all scratch buffers.
- No buffer survives across transaction boundaries.
- This is compatible with concurrent-writer mode: each transaction's cursors are independent.

### Rule 5: Swap-take for serialization buffers
- `make_record_buf` uses the classic Rust swap-take pattern:
  ```rust
  let mut buf = std::mem::take(&mut self.make_record_buf);
  serialize_record_iter_into(iter, &mut buf);
  let blob = std::mem::take(&mut buf);
  self.make_record_buf = buf; // Return empty-but-capacitied buffer
  ```
- This avoids re-allocation on every MakeRecord opcode.

## Concurrency Compatibility

- **Thread-safety:** Cursor buffers are owned by the cursor, which is owned by the
  transaction, which is single-threaded. No synchronization needed.
- **Concurrent-writer mode:** Multiple transactions in different threads each have
  their own cursor sets. No sharing, no contention.
- **Group commit:** The WAL flusher path does NOT touch decode buffers. The
  `commit_wal_group_commit` function only operates on the write_set (staged pages),
  not on cursor state.

## Future Optimization Opportunities

1. **Per-cursor arena for overflow chains:** If profiling shows overflow chain reads
   are a bottleneck (multiple small allocations for page reads), a per-cursor
   bump allocator could batch those. Currently not needed — overflow reads use
   a single Vec with extend_from_slice.

2. **Columnar decode batching:** For wide-table scans, decode all columns in a
   single pass instead of lazy per-column. Currently the lazy bitmap approach
   is optimal for queries that only touch a few columns (the common case).

3. **SmallVec tuning:** The `SmallVec<[SqliteValue; 16]>` threshold (16 columns
   inline) could be tuned based on workload profiling. 16 is a good default for
   typical OLTP schemas.

## Key Files

- `crates/fsqlite-vdbe/src/engine.rs` — VdbeEngine, OP_Column, make_record_buf, results
- `crates/fsqlite-btree/src/cursor.rs` — StorageCursor, payload_into, position_stamp
- `crates/fsqlite-types/src/record.rs` — RecordDecodeScratch, prepare_for_record, invalidate
- `crates/fsqlite-btree/src/overflow.rs` — read_overflow_chain_into
