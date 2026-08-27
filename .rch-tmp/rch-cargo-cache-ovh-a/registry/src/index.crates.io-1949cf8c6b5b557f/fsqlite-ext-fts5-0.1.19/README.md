# fsqlite-ext-fts5

FTS5 full-text search extension for FrankenSQLite.

## Overview

This crate implements the FTS5 full-text search extension, the successor to FTS3/FTS4 with a cleaner API and improved performance. It provides a complete tokenizer framework (unicode61, ascii, porter stemming, trigram), boolean query parsing (implicit AND, OR, NOT, phrase, prefix, NEAR, column filter, caret), an inverted index for document storage and retrieval, BM25 ranking, highlight/snippet auxiliary functions, and a virtual table implementation with content modes (stored and contentless) and secure-delete/contentless-delete configuration.

This crate sits at the extension layer of the fsqlite workspace. It depends on `fsqlite-types`, `fsqlite-error`, `fsqlite-func` (for `ScalarFunction` and `VirtualTable` traits), and `tracing`.

## Shadow-Backed FTS5 Work

The current crate still reflects FrankenSQLite's existing in-memory/materialized FTS5 path. The first-class shadow-backed backend work is tracked by the FTS5 epic and starts from:

- the implementation contract in [`SHADOW_BACKED_FTS5_CONTRACT_MATRIX.md`](./SHADOW_BACKED_FTS5_CONTRACT_MATRIX.md), and
- the target runtime/storage design in [`SHADOW_BACKED_FTS5_TARGET_ARCHITECTURE.md`](./SHADOW_BACKED_FTS5_TARGET_ARCHITECTURE.md), and
- the verification/logging/evidence contract in [`SHADOW_BACKED_FTS5_PROOF_ARTIFACT_CONTRACT.md`](./SHADOW_BACKED_FTS5_PROOF_ARTIFACT_CONTRACT.md).

That matrix is the authoritative spec for:

- stock SQLite `rootpage=0` catalog behavior,
- shadow-table ownership and defensive restrictions,
- `%_config` / `%_content` / `%_docsize` / `%_data` / `%_idx` responsibilities,
- content modes, tokenizer/locale/tokendata semantics, and
- maintenance/integrity command behavior.

The target-architecture document defines the intended runtime object model, MVCC/savepoint invariants, cutover sequence, and the retirement plan for the current materialized positive-rootpage path.

The proof-artifact contract defines the required unit/integration/e2e coverage shape, structured logging fields, replay requirements, manifest conventions, and artifact-bundle expectations for the whole shadow-backed program.

The reusable baseline harness for current FrankenSQLite versus stock SQLite FTS5 lives in `fsqlite-harness::fts5_baseline` and is replayed by `scripts/verify_bd_2nzo8_1_3_fts5_baseline.sh`. It records today’s materialized-path catalog and behavior deltas as machine-readable evidence for `bd-2nzo8.1.3`, including manifest, JSONL events, diff report, benchmark summary, memory/IO summary, replay metadata, and artifact hashes.

## Key Types

- `Fts5Config` - Configuration for an FTS5 virtual table: content mode, secure-delete, and contentless-delete settings
- `ContentMode` - Enum: `Stored` or `Contentless`
- `DeleteAction` - Enum: `Reject`, `Tombstone`, or `PhysicalPurge`
- `Fts5Token` - A single token produced by a tokenizer, with text and byte offset
- `Fts5Tokenizer` (trait) - Tokenizer interface; implementations: `Unicode61Tokenizer`, `AsciiTokenizer`, `PorterTokenizer`, `TrigramTokenizer`
- `Fts5QueryToken` - A parsed token from an FTS5 query expression
- `Fts5QueryTokenKind` - Token classification for FTS5 queries (term, phrase, AND, OR, NOT, NEAR, column filter, prefix, caret, etc.)
- `Fts5QueryError` - Error type for query parsing failures
- `Fts5Expr` - Parsed FTS5 expression tree (Term, Phrase, And, Or, Not, Near, ColumnFilter)
- `InvertedIndex` - In-memory inverted index mapping terms to postings lists with term frequency and position data
- `Posting` - A single posting: document ID, term frequency, and positions
- `Fts5Table` - FTS5 virtual table implementation (implements `VirtualTable`)
- `Fts5Cursor` - Cursor for iterating FTS5 query results (implements `VirtualTableCursor`)
- `Fts5SourceIdFunc` - Scalar function returning the FTS5 source identifier

## Key Functions

- `extension_name()` - Returns `"fts5"`
- `create_tokenizer(name)` - Factory function returning a boxed tokenizer by name
- `parse_fts5_query(query)` - Parse an FTS5 query string into a token sequence
- `build_expr(tokens)` - Build an expression tree from parsed query tokens
- `evaluate_expr(index, expr)` - Evaluate an FTS5 expression against an inverted index, returning matching document IDs
- `bm25_score(...)` - Compute BM25 relevance score for a document
- `highlight(text, terms, open_tag, close_tag)` - Highlight matching terms in text
- `snippet(text, terms, open_tag, close_tag, ellipsis, max_tokens)` - Generate a snippet with highlighted terms
- `register_fts5_scalars(registry)` - Register all FTS5 scalar functions into a function registry

## Dependencies

- `fsqlite-types`
- `fsqlite-error`
- `fsqlite-func`
- `tracing`

## License

MIT
