# Published release consumer checks

This check verifies selected publicly shipped bytes independently of a source
build. Python 3, authenticated read-only `gh api`, and `minisign` on PATH are
required. The repository's pinned epoch-2 public key authenticates the manifest
and payload signatures. The requested tag must peel to the explicit source SHA;
all published asset names must match the signed manifest contract. GitHub
size/digest metadata and before/after release identity must agree.

For example, verify the published Apple Silicon v0.2.3 payload:

```bash
python3 scripts/verify_published_release.py \
  --tag v0.2.3 --commit 2abb303593ea3f5aa7caf840ca548f81c04a9a5c \
  --asset ms-0.2.3-aarch64-apple-darwin \
  --output /path/to/new-evidence-directory --smoke
```

Repeat `--asset` to verify additional platforms. The output directory must not
exist and its parent must already exist. Downloads, metadata, command logs and
`receipt.json` are retained; reruns require a new directory. HTTPS redirects are
checked before following them, and TLS verification remains enabled.

`--smoke` checks only native `--version` and top-level `--help` in a fresh isolated
HOME/XDG environment. No installer, updater, database initialization, indexing,
search mutation or cleanup runs. The `SHA256SUMS.txt` compatibility asset, when
published, must exactly match the signed `SHA256SUMS` manifest and authenticate
with its own signature.

The source was the retained October 2026 publication harness. Its Linux syscall
interposers, bootstrap recovery, fixed worker paths and private fixture layout
are outside this portable check. A passing receipt proves selected signed bytes
and the requested CLI smoke only; it does not establish a fresh-install workflow,
upgrade preservation, indexing/search behavior or release quality gates.
