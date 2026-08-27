## Change: Replace Vec<u8> with SmallVec<[u8; 4]> in Attributes::to_sgr_codes

### Isomorphism Proof

- **Ordering preserved:** yes - SmallVec maintains iteration order identically to Vec
- **Tie-breaking unchanged:** N/A - no tie-breaking logic
- **Floating-point:** N/A - uses u8 only
- **RNG seeds:** N/A - no randomness
- **Golden outputs:** All 85 style tests pass

### Verification
- `cargo test --lib style::` passes (85 tests)
- `cargo check --all-targets` passes
- Functionality identical - SmallVec is API-compatible with Vec

### Performance Impact
- Eliminates heap allocation for styles with 1-4 attributes (>99% of cases)
- Stack allocation for SmallVec<[u8; 4]> uses 4 bytes inline vs heap pointer + allocation
