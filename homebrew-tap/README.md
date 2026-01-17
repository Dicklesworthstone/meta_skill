# Homebrew Tap for ms (Meta Skill)

This is the official Homebrew tap for [ms](https://github.com/Dicklesworthstone/meta_skill) - a local-first skill management platform for AI agents.

## Installation

```bash
# Add the tap
brew tap dicklesworthstone/tap

# Install ms
brew install ms
```

Or install directly in one command:

```bash
brew install dicklesworthstone/tap/ms
```

## Updating

```bash
brew update
brew upgrade ms
```

## Usage

After installation, get started with:

```bash
# Initialize global configuration
ms init --global

# Configure skill paths
ms config skill_paths.project '["./skills"]'

# Index your skills
ms index

# Search for skills
ms search "error handling"

# Get context-aware suggestions
ms suggest
```

For full documentation, see the [ms README](https://github.com/Dicklesworthstone/meta_skill#readme).

## Supported Platforms

| Platform | Architecture | Status |
|----------|--------------|--------|
| macOS | Apple Silicon (arm64) | Supported |
| macOS | Intel (x86_64) | Supported |
| Linux | arm64 | Supported |
| Linux | x86_64 | Supported |

## Formula Maintenance

This tap is automatically updated when new releases are published to the main repository.

### Manual Update

If you need to manually trigger a formula update:

1. Go to Actions > Update Formula
2. Click "Run workflow"
3. Enter the version number (without `v` prefix)
4. A PR will be created with the updated formula

### Contributing

Issues and suggestions related to the Homebrew formula should be filed in this repository.

For issues with ms itself, please file them in the [main repository](https://github.com/Dicklesworthstone/meta_skill/issues).

## License

MIT - see [LICENSE](LICENSE) for details.
