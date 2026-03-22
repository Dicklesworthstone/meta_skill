# Changelog

All notable changes to **ms** (Meta Skill CLI) are documented here.

> Repository: <https://github.com/Dicklesworthstone/meta_skill>
>
> Conventions: entries are organized by landed capabilities, not raw diff order.
> Only `v0.1.0` has a corresponding [GitHub Release](https://github.com/Dicklesworthstone/meta_skill/releases/tag/v0.1.0); `v0.1.1` is a plain git tag (CI build-fix only). Everything after `v0.1.1` is unreleased work on `main`.

---

## [Unreleased] -- HEAD

Tracking period: 2026-01-23 through 2026-03-21 (latest: [`bab33b7`](https://github.com/Dicklesworthstone/meta_skill/commit/bab33b73a7695f1f153a42e05183fbcf07625eab)).

### Rich Output and Terminal Rendering

- **Rich output system expansion** -- messages module, visual snapshots, and full CLI command integration for styled terminal output ([`0191091`](https://github.com/Dicklesworthstone/meta_skill/commit/0191091df64911e8cd64246bb686904e96d777dc))
- **Rich terminal rendering and debug tracing** -- refactored rendering pipeline with tracing instrumentation ([`b8b7562`](https://github.com/Dicklesworthstone/meta_skill/commit/b8b756232b257a5277bcda4045ec0e2e0d4718b2))
- **rich_rust stabilized** -- dependency moved from git pre-release to crates.io v0.2.0 ([`7176369`](https://github.com/Dicklesworthstone/meta_skill/commit/71763693abeebfcb447ffdcb5362f877e1713af5))
- **Toon (tru) integration** -- colored CLI output via toon_rust, later renamed dependency from `toon` to `tru` ([`4632cbd`](https://github.com/Dicklesworthstone/meta_skill/commit/4632cbd016780fb910917d7cbc369501e2264ad2), [`0a89209`](https://github.com/Dicklesworthstone/meta_skill/commit/0a892095a87f1c24a0f23fd88b89fe7d4695e56a))

### Authentication and Distribution

- **Authentication module** -- comprehensive auth system with E2E test coverage ([`ce48e5f`](https://github.com/Dicklesworthstone/meta_skill/commit/ce48e5f960f7364e90ee5f904b4a6cff6cadff98))
- **Installer `--easy-mode`** -- flag for ACFS compatibility in the curl-pipe-bash installer ([`fff4715`](https://github.com/Dicklesworthstone/meta_skill/commit/fff4715f216035884871d743c10d7047e363dda9))

### Bug Fixes

- **CASS API rename** -- `CassSearchResults` field renamed from `matches` to `hits` with backward-compatible alias to avoid keyword collision ([`c185494`](https://github.com/Dicklesworthstone/meta_skill/commit/c185494a8d3a03ba237d31de00ee1e352ddee65b))
- **MCP protocol compliance** -- fixes for Codex compatibility and concurrent access safety ([`0d41756`](https://github.com/Dicklesworthstone/meta_skill/commit/0d41756f82b2002ca5cb58cf8a432dfd09e817b2))
- **Concurrent agent `show` rendering** -- prevent race conditions when multiple agents render the same skill ([`f3d064a`](https://github.com/Dicklesworthstone/meta_skill/commit/f3d064a97deea17e4238cca2343605cec538049d))
- **Installer EXIT trap** -- prevent unbound variable error ([`7f9a4d9`](https://github.com/Dicklesworthstone/meta_skill/commit/7f9a4d91b207c98914f007d03d73d964d5ab8fbf))
- **Test fixtures and beads integration** -- improved robustness ([`cae49d5`](https://github.com/Dicklesworthstone/meta_skill/commit/cae49d58ecb6563aa91be6351978a48bbdf012a6))
- **Compiler warnings suppressed** and test coverage expanded for the testing framework ([`723c51e`](https://github.com/Dicklesworthstone/meta_skill/commit/723c51e700380b71f1041857e2b47abba154b5a1))
- **CASS uncertainty calculation** -- refined logic in cass module ([`3999fab`](https://github.com/Dicklesworthstone/meta_skill/commit/3999fab1074c2fccfc24e40789bba0911dfa7452))
- **ErrorCode import** -- moved to test module where it is actually used ([`e02b039`](https://github.com/Dicklesworthstone/meta_skill/commit/e02b039e694901e3f3b99307f3291ec6dcf9533f))

### Testing

- **8 new E2E test workflow modules** -- expanded command coverage for contract, template, cross-project, evidence, antipatterns, experiment, cm, and recommend commands ([`f53c09c`](https://github.com/Dicklesworthstone/meta_skill/commit/f53c09ce3f409ca4949fe7960ba11a902b77cca6))

### CI / Infrastructure

- **ARM64 Linux binary target** -- `aarch64-unknown-linux-gnu` added to release workflow with AR env var fix ([`ac32c27`](https://github.com/Dicklesworthstone/meta_skill/commit/ac32c27e7009ce4f41fccf27482798315dc6a0cc), [`ec2912f`](https://github.com/Dicklesworthstone/meta_skill/commit/ec2912f529c56e32629d30ab2645642e185f06f0))
- **Dependabot** -- configuration added for automated dependency updates ([`3ff5f8b`](https://github.com/Dicklesworthstone/meta_skill/commit/3ff5f8b83ab071b53bc58595ced9272b059bf761))
- **ACFS notification workflows** -- notify ACFS lesson registry on installer changes ([`5680c58`](https://github.com/Dicklesworthstone/meta_skill/commit/5680c58113ab9370cffb5018ee021d5cc8be9d4d), [`59ab346`](https://github.com/Dicklesworthstone/meta_skill/commit/59ab3467cb8ba37de8a9689a5d6d7ff4b556c64c))

### Chores and Dependency Bumps

- License changed to MIT with OpenAI/Anthropic Rider ([`707002f`](https://github.com/Dicklesworthstone/meta_skill/commit/707002f4d197315024f8474135538740db3726d7))
- GitHub social preview image added ([`dcd3fb5`](https://github.com/Dicklesworthstone/meta_skill/commit/dcd3fb55cabb438dc200f9b7c784ca14c08135ce))
- Beads: 92 unique issue records merged from beads-sync branch ([`87c6c7f`](https://github.com/Dicklesworthstone/meta_skill/commit/87c6c7fa5b10e972ca8dd0bd9493fa65b0dfa664))
- Agent-friendliness report added for ms ([`5e377eb`](https://github.com/Dicklesworthstone/meta_skill/commit/5e377eb2b9e1ca650d0ab51e08f57b0267c4f60b))
- cargo-deny config fixed and unused imports removed ([`fc73fe7`](https://github.com/Dicklesworthstone/meta_skill/commit/fc73fe796df8f7c0fb22b45db6ccf90c371c9772))
- Dependency updates across multiple rounds: rusqlite 0.39.0, toml 1.0.7, chrono 0.4.44, clap 4.5.60, tokio, uuid 1.21.0, tempfile 3.27.0, tracing-subscriber 0.3.23, assert_cmd 2.2.0, which 8.0.2, console 0.16.3, criterion 0.8.2, and GH Actions (upload-artifact v7, download-artifact v19, checkout v6, attest-build-provenance v4, sticky-pull-request-comment v3, repository-dispatch v4)

---

## [v0.1.1] -- 2026-01-23

**Git tag only** -- no GitHub Release. Fixes cross-compilation issues that blocked the v0.1.0 release pipeline.

### Bug Fixes

- **vendored-openssl for musl** -- enable static linking for `x86_64-unknown-linux-musl` target ([`f4121f1`](https://github.com/Dicklesworthstone/meta_skill/commit/f4121f109ac95f9d597c3f49e742cdf04eeb899c))
- **Disable Intel Mac target** -- `x86_64-apple-darwin` removed from CI matrix due to runner issues ([`b77ab2a`](https://github.com/Dicklesworthstone/meta_skill/commit/b77ab2a48e1c554bc4ee468064f0d3e5abab4322))
- **macOS runner update** -- switch to supported runner and disable failing cross-compile targets ([`cfcd2f1`](https://github.com/Dicklesworthstone/meta_skill/commit/cfcd2f12aa3d6883a8b40604b5c063e3d57f68d5))

---

## [v0.1.0] -- 2026-01-23

**First public release.** [GitHub Release](https://github.com/Dicklesworthstone/meta_skill/releases/tag/v0.1.0) with pre-built binaries for Linux (x86_64 glibc), macOS (Apple Silicon), and Windows (x86_64 MSVC). Build provenance attestations included. Also available via Homebrew and Scoop.

This release represents the full initial implementation of the ms platform, built from planning docs to working binary in ~10 days (2026-01-13 through 2026-01-23). It spans 395 commits across all subsystems.

### Core Platform

Dual persistence, hybrid search, and the CLI framework that everything else builds on.

- **Dual persistence** -- SQLite for queries + Git archive for audit trails, with Two-Phase Commit for crash consistency ([`15c3889`](https://github.com/Dicklesworthstone/meta_skill/commit/15c3889c39ba91bfe0ef9576793ebdae6d2588e0), [`9617244`](https://github.com/Dicklesworthstone/meta_skill/commit/9617244c7121c22e3cf187dfdd2eb04b63282672))
- **Hybrid search** -- BM25 via SQLite FTS5 + FNV-1a hash embeddings (384d) fused with Reciprocal Rank Fusion; pluggable embedding backends with API support ([`a5ba168`](https://github.com/Dicklesworthstone/meta_skill/commit/a5ba168e7d225446b4a97c7f5261601289dc1f56), [`b2c0005`](https://github.com/Dicklesworthstone/meta_skill/commit/b2c0005e8503484b9f1d13e5c06888e4169b29ce), [`5b08207`](https://github.com/Dicklesworthstone/meta_skill/commit/5b08207e5ffa520e47432cf884d50a5cc595e37a), [`1e68fa2`](https://github.com/Dicklesworthstone/meta_skill/commit/1e68fa268723a6446c0321ca9f8d1b72c05ed54b))
- **LRU search cache** -- performance optimization for repeated queries ([`aea2a97`](https://github.com/Dicklesworthstone/meta_skill/commit/aea2a97b387dfdf36cf52a8ec6cb08ec0c1dd45a))
- **CLI framework** -- clap v4 with 17 subcommands, robot mode (JSON output), and validation ([`4d83d0a`](https://github.com/Dicklesworthstone/meta_skill/commit/4d83d0aa9a586254a73237ffab60b67bb828b5d5), [`b686d45`](https://github.com/Dicklesworthstone/meta_skill/commit/b686d459a63b47316a259390b03d8b3ee65065dc))
- **Config and error infrastructure** -- layered config (defaults -> global -> project -> env -> flags), structured error types with codes and suggestions ([`029369c`](https://github.com/Dicklesworthstone/meta_skill/commit/029369c5f6b169f9bced150bc6a6425648697876), [`481ec99`](https://github.com/Dicklesworthstone/meta_skill/commit/481ec99831cefb1371028f136d5e14646e68585b))
- **Constrained packing** -- token-budget-aware skill loading with progressive disclosure ([`4ae78d8`](https://github.com/Dicklesworthstone/meta_skill/commit/4ae78d892d16fe5dd7758ec4d43025897009018f), [`7293d55`](https://github.com/Dicklesworthstone/meta_skill/commit/7293d556de337482f468410029ef717ea30a59d6))
- **Pack contracts** -- customizable packing rules (required groups, weights, max-per-group) with built-in presets ([`c18842a`](https://github.com/Dicklesworthstone/meta_skill/commit/c18842a1d7726fb4372208be8c41ba4669834284))

### Security

Multi-layer defense system for AI-assisted workflows.

- **ACIP (Agent Content Injection Prevention)** -- trust boundary classification (user/assistant/tool/file), quarantine system with safe excerpts, replay with explicit acknowledgment, TaintLabel tracking ([`15926b2`](https://github.com/Dicklesworthstone/meta_skill/commit/15926b29a97551fd2f4aa0973f8c9eeee3f67f56), [`cafe094`](https://github.com/Dicklesworthstone/meta_skill/commit/cafe094cbaf77fcbf410c15a7dde8776e135b573), [`392df05`](https://github.com/Dicklesworthstone/meta_skill/commit/392df0545076d1f1c3701e948de04dfda0f3e68c))
- **DCG (Destructive Command Guard)** -- tiered command classification (Safe/Caution/Danger/Critical) with approval gates and safety invariant layer ([`7ab901c`](https://github.com/Dicklesworthstone/meta_skill/commit/7ab901c205f53fbbdea877b0e4eab3b060bf60e0))
- **Path policy** -- symlink escape and directory traversal prevention ([`76e544a`](https://github.com/Dicklesworthstone/meta_skill/commit/76e544ad33c182fcd283909728fb14470156b360))
- **Secret scanning** -- credential and PII detection before content ingest ([`e16cfac`](https://github.com/Dicklesworthstone/meta_skill/commit/e16cfac8d7d0ad27ed8a509bacd0b6c8d4193a93))
- **Whitespace-robust approval matching** -- hardened security approval comparison ([`f97d85e`](https://github.com/Dicklesworthstone/meta_skill/commit/f97d85e7c32dc9afabba49a379c090534e9008ee))

### Skill Authoring and Management

Tools for creating, organizing, inheriting, and composing skills.

- **CASS session mining** -- quality-filtered extraction from AI agent sessions, Specific-to-General Transformation pipeline, uncertainty queue for active learning, session quality scoring ([`b26bf2f`](https://github.com/Dicklesworthstone/meta_skill/commit/b26bf2fe0f336ef8dd21bde5050680b9c35c8234), [`015d658`](https://github.com/Dicklesworthstone/meta_skill/commit/015d6580238405e4d3792dfee9032a2dd2986468), [`61761b8`](https://github.com/Dicklesworthstone/meta_skill/commit/61761b808f03863fe3d707c1f4b3d89440c32869), [`43407a6`](https://github.com/Dicklesworthstone/meta_skill/commit/43407a60811628d4568449dfe9536bf1a6486cbb))
- **Skill inheritance and composition** -- `extends` and `includes` fields for skill reuse, LRU + SQLite resolution cache ([`86da8b7`](https://github.com/Dicklesworthstone/meta_skill/commit/86da8b7d96b5f5143e1b67153fc112f63d1d7d17), [`2af8802`](https://github.com/Dicklesworthstone/meta_skill/commit/2af88021dec84dd1bdfd89eb83949afbc59862a6), [`91423f7`](https://github.com/Dicklesworthstone/meta_skill/commit/91423f77f7a64a01dc15667cdb2b97c4ece95610))
- **Skill formatting and editing** -- markdown normalization (`ms fmt`), semantic diff (`ms diff`), structured editing with round-trip fidelity ([`3b6ef1e`](https://github.com/Dicklesworthstone/meta_skill/commit/3b6ef1eae3b3f7fbf7dadecb9985c9c931d8fed1), [`4fb3b4f`](https://github.com/Dicklesworthstone/meta_skill/commit/4fb3b4f5a908319e5d51b685bf1501bf4df65c99), [`b9b8d52`](https://github.com/Dicklesworthstone/meta_skill/commit/b9b8d52d07d7d70dab9c9be3eb9a01460510904f))
- **Import from documents** -- content parser, block classifiers, and skill generator for extracting skills from arbitrary text ([`d59b4c6`](https://github.com/Dicklesworthstone/meta_skill/commit/d59b4c6da9289dfd8796286a39a247908253a202), [`cc695c8`](https://github.com/Dicklesworthstone/meta_skill/commit/cc695c81e619421a28a2c05e891e96861256f070), [`10f5f1f`](https://github.com/Dicklesworthstone/meta_skill/commit/10f5f1f670036e0288670f3bfb0fa9840ca4c24a))
- **Alias system** -- skill aliases with CLI commands ([`714c1a0`](https://github.com/Dicklesworthstone/meta_skill/commit/714c1a04781c63d28e30ea456dc719753199e806))
- **Layer-aware indexing** -- base/org/project/user skill layers with merge strategies ([`a0746fb`](https://github.com/Dicklesworthstone/meta_skill/commit/a0746fb0c213248675e02d9599586deb2536e1db), [`6b1e614`](https://github.com/Dicklesworthstone/meta_skill/commit/6b1e614facbac8d8e53642caadd092e09a7752dd))
- **Template system** -- curated skill scaffolding templates ([`30ac6e5`](https://github.com/Dicklesworthstone/meta_skill/commit/30ac6e5b4d3ad34eb7de88ead1981673a2c95089))
- **SKILL.md auto-generation** -- create skill files from templates and patterns ([`3b2207c`](https://github.com/Dicklesworthstone/meta_skill/commit/3b2207caa5f3b7f6f461b1dd2727e07b69d26bc2))
- **Evidence and provenance** -- tracking linking skills back to source sessions ([`22655bb`](https://github.com/Dicklesworthstone/meta_skill/commit/22655bbd82cffb634b61488311f903319613fa28))
- **Meta-skill templates** -- built-in templates for common patterns ([`c4ccb02`](https://github.com/Dicklesworthstone/meta_skill/commit/c4ccb029480c0b21bca2d2136b4f0f0fdfab2790))
- **Deduplication engine** -- personalization logic for skill adaptation ([`03b66c8`](https://github.com/Dicklesworthstone/meta_skill/commit/03b66c8edefa672b751343d24d001cef4931e5cc), [`68572168`](https://github.com/Dicklesworthstone/meta_skill/commit/68572168ca37a18883ed0bf9ab455cae02e4f938))

### Adaptive Suggestions and Learning

Bandit optimization and context-aware skill delivery.

- **Thompson sampling bandit** -- UCB exploration, context modifiers, feedback learning for suggestion optimization ([`828d7ac`](https://github.com/Dicklesworthstone/meta_skill/commit/828d7ac22c70796227f28c9d7b8d3881655bca33), [`82b4936`](https://github.com/Dicklesworthstone/meta_skill/commit/82b4936d5503497d09ba7439b569d3a3461ab8c1))
- **Context-aware auto-loading** -- project detection (Rust, Node, Python, Go), file pattern matching, tool detection, relevance scoring ([`8105214`](https://github.com/Dicklesworthstone/meta_skill/commit/8105214a67992e8849ecaa47d9caa5e5686e44c6), [`edbe3e3`](https://github.com/Dicklesworthstone/meta_skill/commit/edbe3e3c930099822ba85ee4015adc3b73b9b109), [`2f10a76`](https://github.com/Dicklesworthstone/meta_skill/commit/2f10a7628b9c7a40f428e41d879d64da13d9291f))
- **Recommendation engine** -- `ms recommend` subcommand with stats, history, and tuning ([`1aefc38`](https://github.com/Dicklesworthstone/meta_skill/commit/1aefc384f5802b435b8743da7e5befa8f9a06190))
- **Feedback collection** -- implicit signal tracking and explicit ratings, skill feedback tracking (schema v9) ([`82b4936`](https://github.com/Dicklesworthstone/meta_skill/commit/82b4936d5503497d09ba7439b569d3a3461ab8c1), [`700073b`](https://github.com/Dicklesworthstone/meta_skill/commit/700073b747edd570b82b054a0b87f4a96a195a6a))
- **User preferences** -- integrated into suggestion system with rich explanations ([`30f1194`](https://github.com/Dicklesworthstone/meta_skill/commit/30f11943a4cd29708004aea51c4061c6f5cdffe4))

### AI Agent Integration

MCP server, agent detection, and inter-agent communication.

- **MCP server** -- stdio and HTTP transports, 6 core tools (search, load, evidence, list, show, doctor), expanded with suggest/feedback/index/validate/config ([`bdf5a58`](https://github.com/Dicklesworthstone/meta_skill/commit/bdf5a58bb94ee4ff9d98add63fc0b9aded0f86f0), [`4cfb467`](https://github.com/Dicklesworthstone/meta_skill/commit/4cfb46772ed2040b3bb91d7bcac2b0ee1104ea11))
- **MCP output safety** -- prevent ANSI codes from leaking into JSON-RPC responses ([`67bb42c`](https://github.com/Dicklesworthstone/meta_skill/commit/67bb42cb21513477079310789418e9816b3d5fa8))
- **MCP lint tool** -- `ms mcp lint` for validating skill quality via MCP ([`54bd298`](https://github.com/Dicklesworthstone/meta_skill/commit/54bd2987f8747ffacae96dd4eeab97013fa85241))
- **Agent auto-detection** -- detect Claude, Codex, and other agents; `ms setup` for zero-config integration ([`f14c373`](https://github.com/Dicklesworthstone/meta_skill/commit/f14c3731247dd506c4fecc79437eba401b555a4e), [`b7c46f0`](https://github.com/Dicklesworthstone/meta_skill/commit/b7c46f030f7b4fcd1d285f31095e1e7a7d6b8dc6))
- **Agent-mail system** -- MCP-based inter-agent communication ([`98ef7d5`](https://github.com/Dicklesworthstone/meta_skill/commit/98ef7d5be25b39d7e9ed8c3d98b70de477bab0d6))
- **Beads flywheel integration** -- BeadsClient for issue tracking and build progress ([`2b9491a`](https://github.com/Dicklesworthstone/meta_skill/commit/2b9491a9aff21fa2a5eebbcbdba578c198f49e43), [`e98914f`](https://github.com/Dicklesworthstone/meta_skill/commit/e98914fb255c3d9c28270880ab03a3edbb5d18a7))

### Graph Analysis and Anti-Patterns

Dependency insights and failure pattern detection.

- **Graph commands** -- dependency insights via bv (beads_viewer): cycles, keystones (PageRank), bottlenecks (betweenness), execution plans, health, export (mermaid/dot/json) ([`d62a630`](https://github.com/Dicklesworthstone/meta_skill/commit/d62a63027548a967eded9acf2d8e53d9cee8a4f4), [`1b9d365`](https://github.com/Dicklesworthstone/meta_skill/commit/1b9d36560c6163e5fa03307516c660f560715439))
- **Anti-pattern mining** -- mine failure patterns from CASS sessions, link to skills, command extraction with ACIP taint tests ([`35f5e90`](https://github.com/Dicklesworthstone/meta_skill/commit/35f5e9097bd2a98fe41cd89d5a4ed1093ba5c5d2), [`3cc2973`](https://github.com/Dicklesworthstone/meta_skill/commit/3cc2973dd67c4032fc547065f79090fda2a058f2))
- **Cross-project learning** -- `ms cross-project` command for pattern aggregation and gap analysis across projects ([`ab6fa32`](https://github.com/Dicklesworthstone/meta_skill/commit/ab6fa320e8113a1a70d7f49150f34437397c666e))

### Multi-Machine Sync

Distributed skill management across machines and remotes.

- **Sync engine** -- Git and filesystem remotes with SSH/token authentication, bidirectional sync with conflict resolution ([`6da250d`](https://github.com/Dicklesworthstone/meta_skill/commit/6da250dc122e09575dd2c001c62eca61b1947706), [`c15a5bf`](https://github.com/Dicklesworthstone/meta_skill/commit/c15a5bf81ffd620a0cd241686324a1699e932a94))
- **JFP Cloud integration** -- JeffreysPrompts Premium Cloud sync support ([`c15a5bf`](https://github.com/Dicklesworthstone/meta_skill/commit/c15a5bf81ffd620a0cd241686324a1699e932a94))
- **RU (Repo Updater) integration** -- skill discovery, auto-reindex, and sync status detection ([`e41407c`](https://github.com/Dicklesworthstone/meta_skill/commit/e41407c44b4a236c2591720212ae0543dabe2b8e), [`7fe943d`](https://github.com/Dicklesworthstone/meta_skill/commit/7fe943d87c86c857d1d15b004d2955b7c3d8d573), [`b8d62e7`](https://github.com/Dicklesworthstone/meta_skill/commit/b8d62e7e923e70c206a75e7b5628879384867d5c))

### Bundle System

Portable skill packages with signing and safety.

- **Bundle packaging** -- create, install, show, and remove bundles with checksums and blob verification ([`23b4bb6`](https://github.com/Dicklesworthstone/meta_skill/commit/23b4bb6d8a7621d00734f0c0ec07c210c3551c82), [`c7398634`](https://github.com/Dicklesworthstone/meta_skill/commit/c7398634f0bf7d86291148db9bf900d6186967c5), [`fc56572`](https://github.com/Dicklesworthstone/meta_skill/commit/fc56572fe840f091c80c89459823d952e959ada8))
- **Ed25519 bundle signing** -- signature creation and verification for trusted distribution ([`8a7e507`](https://github.com/Dicklesworthstone/meta_skill/commit/8a7e50793717769b8d6d15c185ca1cb82c9af862), [`8f76eb2`](https://github.com/Dicklesworthstone/meta_skill/commit/8f76eb2aed36705b8952079519b125d6ed7e9f34))
- **Local modification safety** -- conflict detection, `--force` flag, atomic installation ([`97700ab`](https://github.com/Dicklesworthstone/meta_skill/commit/97700ab0a77c03b09d3a7aa7a0337fdc4b0c6788), [`a55b37a`](https://github.com/Dicklesworthstone/meta_skill/commit/a55b37a18bac3c8dbeb0b5889f654b0dc4ca1f21))
- **Length validation** -- prevent integer overflow attacks in bundle metadata parsing ([`84c3385`](https://github.com/Dicklesworthstone/meta_skill/commit/84c33853ee57b65f70ae278539244dc615bbd42d))

### CLI Output and UX

Unified output system, interactive browser, and CLI polish.

- **OutputFormat system** -- unified output across all commands with TTY detection, robot/plain/rich modes ([`10a7c06`](https://github.com/Dicklesworthstone/meta_skill/commit/10a7c0682706f0f213a01f6ba418ac3484d7990a), [`7bf90e4`](https://github.com/Dicklesworthstone/meta_skill/commit/7bf90e4ae0ee2213114ec58a94ad7b2d86933a9f))
- **rich_rust integration** -- styled terminal output with themes, output builders, ANSI safety for MCP ([`b904d33`](https://github.com/Dicklesworthstone/meta_skill/commit/b904d3323cd9a2c31d744e0015d50709c8064932), [`403118b`](https://github.com/Dicklesworthstone/meta_skill/commit/403118ba6a33b61797671d14b649fd15d1818290))
- **Interactive skill browser** -- `ms browse` TUI with pattern search filtering ([`a2fe485`](https://github.com/Dicklesworthstone/meta_skill/commit/a2fe485239c873ab70fa0b0224bbc9cc86388237), [`49c114a`](https://github.com/Dicklesworthstone/meta_skill/commit/49c114a8f2d694e538ac256c81f909d5e1b7a31d))
- **Progress reporter** -- TTY-aware with multi-mode support ([`1d7cff2`](https://github.com/Dicklesworthstone/meta_skill/commit/1d7cff2c88d891e23f80f16a9749ac38fa6aaa58))
- **Unified color module** -- semantic palette with TTY detection ([`7bf90e4`](https://github.com/Dicklesworthstone/meta_skill/commit/7bf90e4ae0ee2213114ec58a94ad7b2d86933a9f))

### Quality, Validation, and Experimentation

Lint framework, A/B testing, and quality scoring.

- **Lint framework** -- `ValidationRule` trait with structural, reference, security, quality, and performance rules ([`bbcac6c`](https://github.com/Dicklesworthstone/meta_skill/commit/bbcac6cf2a70744500dbfe372dea0fc9cddc102f), [`ee83615`](https://github.com/Dicklesworthstone/meta_skill/commit/ee836153907946729b0daea5e877f83c6cb35b34), [`3c5742e`](https://github.com/Dicklesworthstone/meta_skill/commit/3c5742e14570286a6b1ee399a2fb54581564d3be))
- **UBS client** -- integration with ultimate_bug_scanner for static analysis validation ([`57741e1`](https://github.com/Dicklesworthstone/meta_skill/commit/57741e1f8665cce5c254607c842596ed1f0e593e))
- **A/B experiment system** -- full lifecycle: create, assign, record, status (z-test), conclude, load variant ([`b92a4c8`](https://github.com/Dicklesworthstone/meta_skill/commit/b92a4c82b6e4cda86e29b09457355cf3d56e6e96))
- **Skill test framework** -- run tests defined in skill metadata ([`ac5ebba`](https://github.com/Dicklesworthstone/meta_skill/commit/ac5ebbae8ca185f244a97d60a61e469257b427d7))
- **Prune system** -- pruning analysis, interactive review, merge/deprecate/split proposals, beads issue emission ([`9b7a711`](https://github.com/Dicklesworthstone/meta_skill/commit/9b7a711e1b7266ac309cd799f8c81211cd72189e), [`690451f`](https://github.com/Dicklesworthstone/meta_skill/commit/690451f771300455481d215b06d80a18072e6f2c))
- **Sandboxed skill execution** -- simulation environment for testing skill commands ([`b042b13`](https://github.com/Dicklesworthstone/meta_skill/commit/b042b1379c689b4705ec2cc4545747ab99599704))
- **Skill usage tracking** -- infrastructure for monitoring which skills are used ([`c9727c5`](https://github.com/Dicklesworthstone/meta_skill/commit/c9727c50d8a792d99d2b4bfa7d2e0a354c40d066))

### CASS Memory Integration

Connection to the Cross-Agent Session Search system and cass-memory (cm).

- **CM client** -- methods and tests for skill mining integration, `ms cm` commands for context, rules, and similarity ([`58361e6`](https://github.com/Dicklesworthstone/meta_skill/commit/58361e6897c5b373026fab9ed20a52811ab299c8), [`2190cfb`](https://github.com/Dicklesworthstone/meta_skill/commit/2190cfbba7ae08f9c6227e537ee849d3e1425495))
- **CM client update** -- updated cass-memory client integration ([`a5af421`](https://github.com/Dicklesworthstone/meta_skill/commit/a5af421e94d83f30e718751656a99771de3c9437))

### Build and Distribution

Install scripts, package managers, and auto-update.

- **Auto-update system** -- `ms update --check` with GitHub release detection and pinned version support ([`d961f48`](https://github.com/Dicklesworthstone/meta_skill/commit/d961f48b8ee3fc43188392bf4c7f17cd45892373), [`7339d9d`](https://github.com/Dicklesworthstone/meta_skill/commit/7339d9d04aaaea779490a19f2df2db33756e053a))
- **Cross-platform install script** -- shell installer for Linux, macOS, Windows ([`323e15f`](https://github.com/Dicklesworthstone/meta_skill/commit/323e15f5be70753590b0cde309b6321e311fb269))
- **Homebrew tap** -- template and release integration ([`2a080eb`](https://github.com/Dicklesworthstone/meta_skill/commit/2a080eb8987539d4f5dd99a17e72c0a490d970f0))
- **Scoop bucket** -- template and release integration ([`9f8d560`](https://github.com/Dicklesworthstone/meta_skill/commit/9f8d56046fdf5b5f282a2d5a485701ab02de024b))
- **rustls over native-tls** -- for reliable cross-compilation ([`08b69f7`](https://github.com/Dicklesworthstone/meta_skill/commit/08b69f7e3d9dbefff48ae2c4ccb569277cb3b0e3))
- **Auto build pipeline** -- BuildSession state machine with checkpoint resume and uncertainty resolution ([`84641285`](https://github.com/Dicklesworthstone/meta_skill/commit/84641285b5cfa6c15c3458b691785e2c51c76299), [`e516544`](https://github.com/Dicklesworthstone/meta_skill/commit/e516544721972087ca1ac2686084afa63050386a))

### Performance

- **Bulk git history lookup** -- O(1) modification times for indexing ([`314c45b`](https://github.com/Dicklesworthstone/meta_skill/commit/314c45b02e7bc1212aab8c56dbc9cc3c8756ee40))
- **Token packing optimization** -- improved heuristic to avoid local minima ([`27eb780`](https://github.com/Dicklesworthstone/meta_skill/commit/27eb7805239a1a8651af96b58e0da0c4b41818fa), [`0e33279`](https://github.com/Dicklesworthstone/meta_skill/commit/0e3327900528bd369d2faddc24384dbea3901b35))
- **Cached parsed skills** -- during meta-skill load ([`fbf9e46`](https://github.com/Dicklesworthstone/meta_skill/commit/fbf9e46e0da0036ea36e4e5ab20c38ea93cfeea7))
- **Benchmark suite** -- Criterion benchmarks with performance targets for search, indexing, and suggestions ([`6e61531`](https://github.com/Dicklesworthstone/meta_skill/commit/6e615318dc8b69e6fd037c81ef7648ff3a4a4b93), [`f6afcda`](https://github.com/Dicklesworthstone/meta_skill/commit/f6afcdaa1338668efe3ee324346aa20930bdc69d))

### CI / Infrastructure

- **GitHub Actions CI/CD** -- CI workflow, release workflow with build provenance, benchmark workflow with Criterion ([`5967df0`](https://github.com/Dicklesworthstone/meta_skill/commit/5967df0d2bcd9b09d3ce6fe47a6e15c9510befe9), [`6d1c133`](https://github.com/Dicklesworthstone/meta_skill/commit/6d1c133fbd74fccb7f2ce74517b4f9e8f4cae6cc), [`28f1bf2`](https://github.com/Dicklesworthstone/meta_skill/commit/28f1bf25c0dafcb4f7daa52a8578969d1351a280))
- **Modern CI best practices** -- improved GitHub Actions workflows ([`efef7e3`](https://github.com/Dicklesworthstone/meta_skill/commit/efef7e3c3b64384e40a66a05c1ab11d48decd0b5))

### Testing

Comprehensive test infrastructure spanning unit, integration, E2E, property, and benchmark tests.

- **Unit test suites** for: bundle, search, index, load, suggest, doctor, prune, config, migrations, utils, updater, beads types, safety, quality, graph, meta_skills, agent_mail, context detection modules
- **E2E workflow tests** for: skill discovery, bundle install, safety, security, sync, CASS, beads integration, rich output, agent compatibility workflows
- **Property tests** with proptest for fuzzing edge cases
- **Test infrastructure** -- TestFixture, wiremock mock server, MockBeadsClient, `.msb` fixture files, comprehensive test logging ([`bd50c27`](https://github.com/Dicklesworthstone/meta_skill/commit/bd50c27908143f037162abe29e2523ac66a8f416), [`be346af`](https://github.com/Dicklesworthstone/meta_skill/commit/be346afaf89da86627ccc301c6e56c2114956e70), [`131aacb`](https://github.com/Dicklesworthstone/meta_skill/commit/131aacb8070a6a0cae7172069977c571fb8a16f4), [`b7f9d82`](https://github.com/Dicklesworthstone/meta_skill/commit/b7f9d82ef0e4f6ba139c81aa9be48fcae818dbd8))

### Notable Bug Fixes (v0.1.0)

- Multiple logic bugs found in fresh-eyes code review ([`d5f2652`](https://github.com/Dicklesworthstone/meta_skill/commit/d5f2652582e5980759af4b3acdd0ba7b81e1334a))
- Brenner wizard state machine critical bugs ([`d6def59`](https://github.com/Dicklesworthstone/meta_skill/commit/d6def592a3b87ca5e6e10aec8bb4a82c95be7db9))
- JSON-RPC 2.0 notification handling in MCP ([`11e8827`](https://github.com/Dicklesworthstone/meta_skill/commit/11e882741bf2792f9a011707ea51aabfb6d148df))
- Integer overflow in suggestion cooldown calculation ([`6e9be2e`](https://github.com/Dicklesworthstone/meta_skill/commit/6e9be2e39b59e67a2c1dfd7ec1a18475dc271971))
- Sync data loss prevention -- conditionally updating base hash ([`a56ca1c`](https://github.com/Dicklesworthstone/meta_skill/commit/a56ca1caea71dc502afff7290202ba6f4e237a19))
- Horizontal rule parsing in skill markdown ([`54bd298`](https://github.com/Dicklesworthstone/meta_skill/commit/54bd2987f8747ffacae96dd4eeab97013fa85241))
- UTF-8 safe string truncation in antipatterns ([`270220e`](https://github.com/Dicklesworthstone/meta_skill/commit/270220e737a596adf3e862b27a9e74afcc6e6af2))
- Cross-platform atomic file replace operations ([`252dda5`](https://github.com/Dicklesworthstone/meta_skill/commit/252dda5b386249791a8d53903c3a428cc549a6ad))
- RAII terminal guard for panic-safe TUI state recovery ([`c5b3676`](https://github.com/Dicklesworthstone/meta_skill/commit/c5b3676b5c1292acff9ad512fc47403664a95687))
- Deterministic dependency graph and topological sort ([`17eb8dc`](https://github.com/Dicklesworthstone/meta_skill/commit/17eb8dcdeb04edc82e5c199ed783767a9a1bef22))
- Token count persistence in SQLite during indexing ([`8e5e45d`](https://github.com/Dicklesworthstone/meta_skill/commit/8e5e45d0a244e2573dbfc1cca2fe4e2d4186b746))
- Hash embedding dimension indexing correction ([`7c32c75`](https://github.com/Dicklesworthstone/meta_skill/commit/7c32c757004d67ca5b84cb4c3c2398a94a313d72))

### Miscellaneous

- **Planning phase** (Jan 13) -- 39 design sections covering architecture, CASS mining, Brenner Method, APR refinement, performance profiling, optimization, security assessment, error handling, testing, CI/CD, caching, debugging, and REST API patterns ([`076b097`](https://github.com/Dicklesworthstone/meta_skill/commit/076b0976a77f0380b31ff98dddf3548726c44629) through [`989535d`](https://github.com/Dicklesworthstone/meta_skill/commit/989535d0e673c0133d14d53746aa1b812ea61559))
- **Beads implementation plan** -- 100 enriched beads tracking the full build ([`c526794`](https://github.com/Dicklesworthstone/meta_skill/commit/c526794297fba6569a95e444d43ea250c869abe7) through [`f5467f3`](https://github.com/Dicklesworthstone/meta_skill/commit/f5467f3e8278d2724aaf79ae5f7e6b99cc1abc09))
- MIT License added ([`183e8c3`](https://github.com/Dicklesworthstone/meta_skill/commit/183e8c39de04e281226aaa93a7f770cc15ab1963))
- **once_cell migration** -- migrated to `std::sync::LazyLock` ([`41eab4a`](https://github.com/Dicklesworthstone/meta_skill/commit/41eab4a02c121c42019037777fc7b9fab1c7288c))

### Release Assets (v0.1.0)

| Target | File | Downloads |
|--------|------|-----------|
| Linux x86_64 (glibc) | `ms-0.1.0-x86_64-unknown-linux-gnu.tar.gz` | 804 |
| macOS Apple Silicon | `ms-0.1.0-aarch64-apple-darwin.tar.gz` | 73 |
| Windows x86_64 | `ms-0.1.0-x86_64-pc-windows-msvc.zip` | 3 |

---

## Pre-history -- 2026-01-13

Initial commit with planning documents. No Rust code yet.

- [`076b097`](https://github.com/Dicklesworthstone/meta_skill/commit/076b0976a77f0380b31ff98dddf3548726c44629) -- Initial commit: meta_skill (ms) CLI planning docs

---

[Unreleased]: https://github.com/Dicklesworthstone/meta_skill/compare/v0.1.1...HEAD
[v0.1.1]: https://github.com/Dicklesworthstone/meta_skill/compare/v0.1.0...v0.1.1
[v0.1.0]: https://github.com/Dicklesworthstone/meta_skill/releases/tag/v0.1.0
