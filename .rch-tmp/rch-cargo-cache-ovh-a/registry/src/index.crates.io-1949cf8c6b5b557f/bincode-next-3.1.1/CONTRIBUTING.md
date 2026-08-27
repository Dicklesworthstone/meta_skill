# Contributing to Bincode-Next

First off, thank you for considering contributing to Bincode-Next! It's people like you who make the Rust community great.

## Code of Conduct

We are committed to providing a welcoming and inspiring community. We do not tolerate harassment, doxxing, or any form of hate speech. Please be respectful to all contributors and the original authors of the library. Please see the [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for more details.

## Performance First

Bincode-Next is built for **extreme performance**. Any PR that affects the hot-path (encoding/decoding loops, varint handling, etc.) **must** include benchmarks.

- **Benchmarks**: Use `criterion` for macro-benchmarks and ensure that performance is either improved or unaffected (no regressions).
- **Miri**: All `unsafe` code must be validated with Miri (`MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --all-features --no-fail-fast`). We prioritize memory safety alongside performance.
- **SIMD**: When adding SIMD optimizations:
  - Provide a safe fallback for non-supported architectures.
  - Use feature detection or target-based dispatch.
  - Keep internal SIMD logic in the `src/varint/simd.rs` or similar specialized modules.

## Technical Standards

- **Rust Version**: We maintain an MSRV (Minimum Supported Rust Version). Please check `Cargo.toml` or `README.md` before using very new language features.
- **Documentation**: All new public APIs must be documented with examples and non-breaking changes are prefered.
- **Specification**: If your change affects the binary format, you **must** update `docs/spec.md` and write a clear pull request message to explain the reasons.

## Workflow

1. **Fork the repository** and create your branch from branch `main`.
2. **Implement your changes** and add tests.
3. **Run the test suite**:
   ```bash
   cargo test --all-features --all-targets
   cargo clippy --all-features
   MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test --all-features --no-fail-fast
   ```
4. **Benchmark if applicable**:
   ```bash
   cargo bench --all-features
   ```
5. **Submit a Pull Request** with a clear description of your changes and performance results.

## AI-Assisted Development

We welcome discussions and contributions generated with the help of AI systems. However, as a contributor, you are responsible for verified accuracy, safety, and performance of any AI-suggested code.

## License

By contributing, you agree that your contributions will be dual licensed under the MIT License and the Apache License, Version 2.0.
