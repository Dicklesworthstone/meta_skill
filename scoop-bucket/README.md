# Scoop Bucket for ms (Meta Skill)

This is the official Scoop bucket for [ms](https://github.com/Dicklesworthstone/meta_skill) - a local-first skill management platform for AI agents.

## Installation

```powershell
# Add the bucket
scoop bucket add ms https://github.com/Dicklesworthstone/scoop-bucket

# Install ms
scoop install ms/ms
```

Or install directly in one command:

```powershell
scoop install https://raw.githubusercontent.com/Dicklesworthstone/scoop-bucket/main/bucket/ms.json
```

## Updating

```powershell
scoop update
scoop update ms
```

## Usage

After installation, get started with:

```powershell
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
| Windows | x64 | Supported |
| Windows | ARM64 | Planned |

## Bucket Maintenance

This bucket is automatically updated when new releases are published to the main repository.

### Manual Update

If you need to manually trigger a manifest update:

1. Go to Actions > Update Manifests
2. Click "Run workflow"
3. Optionally enter the version number (without `v` prefix)
4. A PR will be created with the updated manifest

### Contributing

Issues and suggestions related to the Scoop bucket should be filed in this repository.

For issues with ms itself, please file them in the [main repository](https://github.com/Dicklesworthstone/meta_skill/issues).

## License

MIT - see [LICENSE](LICENSE) for details.
