# Extended Section Templates

Additional templates for specialized README sections.

---

## Performance Section

For tools where speed matters:

```markdown
## Performance

`tool-name` is designed for speed:

| Operation | Time | Notes |
|-----------|------|-------|
| Startup | <50ms | Lazy initialization |
| Search | <10ms | Memory-mapped indices |
| Indexing | 1000 docs/sec | Parallel processing |

### Benchmarks

On a typical workload (10,000 items):

| Operation | tool-name | Alternative A | Alternative B |
|-----------|-----------|---------------|---------------|
| Cold start | 45ms | 230ms | 890ms |
| Warm query | 3ms | 15ms | 120ms |
| Memory (idle) | 12MB | 45MB | 200MB |

*Tested on M2 MacBook Pro, 16GB RAM. Your results may vary.*

### Optimizations

- **Lazy initialization**: Resources compiled on first use
- **Memory-mapped files**: OS-level caching
- **Parallel processing**: Multi-core utilization via rayon
- **Incremental updates**: Only process changed items
```

---

## Security Section

For tools handling sensitive data:

```markdown
## Security

### Your Data Never Leaves Your Machine

`tool-name` is designed with privacy as a non-negotiable:

```
┌─────────────────────────────────────────────────────────────┐
│                     YOUR MACHINE                            │
│                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐      │
│  │   Input     │───▶│   Process   │───▶│   Output    │      │
│  └─────────────┘    └─────────────┘    └─────────────┘      │
│                                                             │
│  ❌ No network calls                                        │
│  ❌ No telemetry                                            │
│  ❌ No cloud sync                                           │
│  ❌ No API keys required                                    │
└─────────────────────────────────────────────────────────────┘
```

### What's Stored Where

| Location | Contents | Sensitive? |
|----------|----------|------------|
| `~/.tool/db.sqlite` | Metadata, indices | ⚠️ Yes |
| `~/.tool/cache/` | Temporary files | Low |
| Config file | Settings only | No |

### Secure Deletion

```bash
# Remove all tool data
rm -rf ~/.tool/

# Verify removal
ls ~/.tool/  # Should not exist
```
```

---

## Data Model Section

For tools with complex data structures:

```markdown
## Data Model

### What Gets Indexed

| Field | Indexed | Stored | Notes |
|-------|---------|--------|-------|
| `id` | ✅ Term | ✅ | Primary key |
| `content` | ✅ Full-text | ✅ | Searchable |
| `created_at` | ✅ Date | ✅ | For filtering |
| `metadata` | ❌ | ✅ | JSON blob |

### Schema

```sql
CREATE TABLE items (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    metadata JSON
);

CREATE INDEX idx_created ON items(created_at);
CREATE VIRTUAL TABLE items_fts USING fts5(content);
```
```

---

## API Reference Section

For libraries with programmatic APIs:

```markdown
## API Reference

### Basic Usage

```rust
use tool_name::Client;

let client = Client::new()?;
let results = client.search("query")?;
```

### Configuration

```rust
let client = Client::builder()
    .with_timeout(Duration::from_secs(30))
    .with_retry(3)
    .build()?;
```

### Error Handling

```rust
match client.operation() {
    Ok(result) => println!("Success: {:?}", result),
    Err(Error::NotFound) => println!("Item not found"),
    Err(Error::Timeout) => println!("Operation timed out"),
    Err(e) => return Err(e.into()),
}
```

### Types

| Type | Description |
|------|-------------|
| `Client` | Main API client |
| `Config` | Configuration options |
| `Result<T>` | Operation result |
| `Error` | Error variants |
```

---

## Migration/Upgrade Section

For tools with breaking changes:

```markdown
## Upgrading

### From v1.x to v2.x

**Breaking changes:**
- Config file location changed from `~/.toolrc` to `~/.config/tool/config.toml`
- Command `tool old-cmd` renamed to `tool new-cmd`
- Flag `--old-flag` removed, use `--new-flag` instead

**Migration steps:**

```bash
# 1. Backup existing config
cp ~/.toolrc ~/.toolrc.backup

# 2. Install v2
curl -fsSL https://... | bash

# 3. Migrate config
tool migrate --from v1

# 4. Verify
tool doctor
```

### From v2.x to v3.x

No breaking changes. Upgrade in place:

```bash
tool update
```
```

---

## Contributing Section (Expanded)

For projects accepting contributions:

```markdown
## Contributing

### Development Setup

```bash
# Clone
git clone https://github.com/user/repo.git
cd repo

# Install dependencies
cargo build

# Run tests
cargo test

# Run lints
cargo clippy --all-targets -- -D warnings
```

### Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`cargo test`)
5. Run lints (`cargo clippy`)
6. Commit (`git commit -m 'Add amazing feature'`)
7. Push (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- All public APIs must have doc comments
- Tests required for new features
- No warnings from clippy

### Commit Messages

Format: `type(scope): description`

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
```
feat(search): add fuzzy matching support
fix(parser): handle empty input gracefully
docs(readme): add troubleshooting section
```
```

---

## Ecosystem/Integration Section

For tools in a larger ecosystem:

```markdown
## Ecosystem Integration

`tool-name` integrates with the following tools:

| Tool | Integration | Use Case |
|------|-------------|----------|
| **Tool A** | Native | Primary workflow |
| **Tool B** | Via CLI | Automation |
| **Tool C** | Plugin | IDE integration |

### With Tool A

```bash
# tool-name provides data to Tool A
tool export --format tool-a | tool-a import
```

### With Tool B

```bash
# Use tool-name output in Tool B pipelines
tool search "query" --format json | tool-b process
```

### IDE Integration

- **VS Code**: Install extension `tool-name-vscode`
- **JetBrains**: Plugin available in marketplace
- **Vim/Neovim**: See [vim-tool-name](https://github.com/user/vim-tool-name)
```

---

## Environment Variables Reference

```markdown
## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TOOL_HOME` | Data directory | `~/.tool` |
| `TOOL_CONFIG` | Config file path | `~/.config/tool/config.toml` |
| `TOOL_LOG_LEVEL` | Logging verbosity | `info` |
| `TOOL_NO_COLOR` | Disable colored output | unset |
| `TOOL_OFFLINE` | Disable network access | unset |

### Example

```bash
# Use custom data directory
export TOOL_HOME=/data/tool

# Enable debug logging
export TOOL_LOG_LEVEL=debug

# Run tool
tool command
```
```

---

## Shell Completion Section

```markdown
## Shell Completions

### Bash

```bash
# Add to ~/.bashrc
eval "$(tool completions bash)"

# Or generate file
tool completions bash > ~/.local/share/bash-completion/completions/tool
```

### Zsh

```bash
# Add to ~/.zshrc
eval "$(tool completions zsh)"

# Or generate file
tool completions zsh > ~/.zfunc/_tool
```

### Fish

```bash
tool completions fish > ~/.config/fish/completions/tool.fish
```

### PowerShell

```powershell
tool completions powershell | Out-String | Invoke-Expression
```
```

---

## Release Notes Pattern

For CHANGELOG or release sections:

```markdown
## Release Notes

### v2.1.0 (2026-01-15)

**New Features:**
- Added `tool new-command` for X functionality
- Support for Y file format

**Improvements:**
- 40% faster search performance
- Better error messages for common failures

**Bug Fixes:**
- Fixed crash when input contains unicode
- Resolved memory leak in long-running sessions

**Breaking Changes:**
- None

### v2.0.0 (2025-12-01)

**Breaking Changes:**
- Config file format changed from JSON to TOML
- Removed deprecated `--old-flag`

See [Migration Guide](#upgrading) for upgrade instructions.
```

---

## Acknowledgments Section

```markdown
## Acknowledgments

Built with:
- [Rust](https://www.rust-lang.org/) — Systems programming language
- [Tantivy](https://github.com/quickwit-oss/tantivy) — Full-text search engine
- [SQLite](https://sqlite.org/) — Embedded database
- [clap](https://github.com/clap-rs/clap) — Command-line argument parsing

Inspired by:
- [ripgrep](https://github.com/BurntSushi/ripgrep) — Fast grep alternative
- [bat](https://github.com/sharkdp/bat) — Cat with wings

Special thanks to all [contributors](https://github.com/user/repo/graphs/contributors).
```
