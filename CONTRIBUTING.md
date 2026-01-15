# Contributing to meta_skill

## Performance Profiling

To profile `ms` using flamegraphs:

1.  Install `flamegraph`:
    ```bash
    cargo install flamegraph
    ```

2.  Build with profiling support:
    ```bash
    cargo build --profile=profiling
    ```

3.  Run profiling:
    ```bash
    cargo flamegraph --bin ms -- search "your query"
    ```

4.  View `flamegraph.svg` in a browser.
