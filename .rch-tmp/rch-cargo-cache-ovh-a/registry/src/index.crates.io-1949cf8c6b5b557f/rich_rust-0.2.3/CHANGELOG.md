# Changelog

All notable changes to rich_rust are documented in this file.

Organized by capabilities, not raw diff order. Each version links to its
GitHub Release (where one exists) and includes representative commit links
(`https://github.com/Dicklesworthstone/rich_rust/commit/<hash>`).

Tag types: v0.1.0 and v0.1.1 are annotated tags with release messages;
v0.2.0 and v0.2.1 are lightweight tags. All four have corresponding
GitHub Releases.

---

## [Unreleased]

Work merged to `main` after the v0.2.1 tag.

- Documentation: add cass (Cross-Agent Session Search) tool reference to AGENTS.md ([478c1be](https://github.com/Dicklesworthstone/rich_rust/commit/478c1be6bb701d583608057e1f1b882119fbaf56))

---

## [0.2.1] -- 2026-02-22

[GitHub Release](https://github.com/Dicklesworthstone/rich_rust/releases/tag/v0.2.1) (Latest) · lightweight tag · 9 commits since v0.2.0

Patch release that removes the nightly-only feature gate (enabling stable
Rust edition 2024 builds), upgrades dependencies, fixes syntax
highlighting in interactive prompts, and updates the license.

### Compatibility

- Remove nightly-only feature gate so the crate builds on stable Rust 2024 edition ([8785f20](https://github.com/Dicklesworthstone/rich_rust/commit/8785f20fa7794829f534ec017c455be3f4df66fa))

### Bug Fixes

- Enable syntax highlighting in `Prompt` and `Select` prompt rendering ([b155a2d](https://github.com/Dicklesworthstone/rich_rust/commit/b155a2d858a08098a5719cc206cfefd0ca021820))
- Replace `assert!(false, ...)` with `panic!()` in conformance tests for correct behavior under `#[should_panic]` ([c109de9](https://github.com/Dicklesworthstone/rich_rust/commit/c109de974faa577e9e41842a2ac78790e79d54bc))

### Dependencies

- Upgrade bitflags, fancy-regex, time, proptest, and others ([9556469](https://github.com/Dicklesworthstone/rich_rust/commit/955646965ee43a060da36954dbc22295d01f9668))

### Licensing and Metadata

- Update license to MIT with OpenAI/Anthropic Rider ([b9e75ef](https://github.com/Dicklesworthstone/rich_rust/commit/b9e75efe648a54207b032fe4b6f91cadb5b4b36a))
- Update README license references to match ([8af0ed9](https://github.com/Dicklesworthstone/rich_rust/commit/8af0ed93ed8c4fc707cc183243adbd978df3918e))
- Add GitHub social preview image (1280x640) ([e7beb53](https://github.com/Dicklesworthstone/rich_rust/commit/e7beb539cd4bcdad7dc5159a5c46259a1791ec88))

### Housekeeping

- Remove tracked `__pycache__/*.pyc` files ([2eb80a8](https://github.com/Dicklesworthstone/rich_rust/commit/2eb80a8d7e15e23d5879d128e940eb9fabd96f46))
- Add `.DS_Store` to `.gitignore` ([ecfad41](https://github.com/Dicklesworthstone/rich_rust/commit/ecfad413b47890e105b5212de699a84be50fa01d))

---

## [0.2.0] -- 2026-02-15

[GitHub Release](https://github.com/Dicklesworthstone/rich_rust/releases/tag/v0.2.0) · lightweight tag · 221 commits since v0.1.1

Major milestone release that substantially closes the gap with Python Rich
feature parity. Adds seven new renderables/modules, a complete interactive
demo showcase ("Nebula Deploy"), conformance testing against Python Rich
fixtures, and significant performance work.

### New Renderables

- **Traceback** renderable with syntax-highlighted source context and
  optional runtime backtrace capture via `Traceback::capture` (feature
  `backtrace`). Source indentation preserved; "main" is no longer
  incorrectly filtered as an internal frame.
  ([b35a58f](https://github.com/Dicklesworthstone/rich_rust/commit/b35a58f92b04dabfb0fbec14022cd36ba7d73d6f),
  [6bd5d4b](https://github.com/Dicklesworthstone/rich_rust/commit/6bd5d4ba12a4472b890219051acb7ecbd089ca77),
  [540595e](https://github.com/Dicklesworthstone/rich_rust/commit/540595ee1e9631a3a0e3ea97e4f84a6794229a1b))
- **Control** renderable and control code helpers for cursor movement,
  erase, and terminal control (`rich.control` parity with conformance
  fixtures).
  ([88bda62](https://github.com/Dicklesworthstone/rich_rust/commit/88bda62e2a653d01eccee43260fc7e85072ac8f0),
  [3fa016c](https://github.com/Dicklesworthstone/rich_rust/commit/3fa016c07934bfb1efa522063e3da02be6a88f23))
- **Constrain** renderable for clamping renderable width
  (`rich.constrain` parity).
  ([24126e5](https://github.com/Dicklesworthstone/rich_rust/commit/24126e5c0a8b305979d3607bdf183c517516fa3b))
- **Group** renderable and `Text::join` method for composing multiple
  renderables into a single output stream.
  ([4a952d3](https://github.com/Dicklesworthstone/rich_rust/commit/4a952d33a95790fe5a53a6b25c3f459f3a76a338))
- **Pretty/Inspect** renderables for `Debug`-based object introspection
  with nested-structure support.
  ([92c1b5e](https://github.com/Dicklesworthstone/rich_rust/commit/92c1b5e2c6734cfbbf2abfbc710518ba2a87d846),
  [870a893](https://github.com/Dicklesworthstone/rich_rust/commit/870a893a31c81b29221fe9194d4b987759d798ca),
  [1080a6b](https://github.com/Dicklesworthstone/rich_rust/commit/1080a6b6886e0c95769a08e05080557db1f9bb30))

### New Modules

- **Highlighter** module with full Python Rich highlighter parity (regex
  highlighters, `ReprHighlighter`, borrow-split fix in `apply_regex`).
  ([e4a9986](https://github.com/Dicklesworthstone/rich_rust/commit/e4a9986075bd325fd7476d22728a2ff497c326ac),
  [2901aad](https://github.com/Dicklesworthstone/rich_rust/commit/2901aad9e3023df80f70ccd6143c5b023a8490fd),
  [1523883](https://github.com/Dicklesworthstone/rich_rust/commit/1523883082724f543d511168ccfa4fd2009a7286))
- **Emoji** module ported from Python Rich emoji codes.
  ([6b45c59](https://github.com/Dicklesworthstone/rich_rust/commit/6b45c59fe9a1b0af9afd86b3df0582f7a6519659))
- **Pager** module for graceful long-content paging.
  ([f990963](https://github.com/Dicklesworthstone/rich_rust/commit/f9909637dd05ce3e2372cf588138d730066e908e))
- **File size formatting** (`filesize` module) and transfer speed display
  columns (`FileSizeColumn`, `DownloadColumn`, `TransferSpeedColumn`) for
  progress bars.
  ([f97606f](https://github.com/Dicklesworthstone/rich_rust/commit/f97606fb7cb25fde5f3e67fa6e3fc9eabe51717f),
  [a5c9aa8](https://github.com/Dicklesworthstone/rich_rust/commit/a5c9aa8ab2670640a8aa6a3e5c8b122129118088),
  [79ee7dc](https://github.com/Dicklesworthstone/rich_rust/commit/79ee7dc81e3c0fccac2f94cecf569d97bc7cfda0))

### Interactive and Console Enhancements

- **Select** prompt for choice selection.
  ([0085b97](https://github.com/Dicklesworthstone/rich_rust/commit/0085b9771c40c0ce116e7e753a25e2b145a154f3))
- **Theme** system for named styles matching Python Rich
  (`rule.line`, `table.header`, custom names).
  ([fe0cb8a](https://github.com/Dicklesworthstone/rich_rust/commit/fe0cb8a5782ddd2b0f9dedef672971aad61dbda8))
- **ANSI decode** parity: `Text::from_ansi` + `AnsiDecoder` for parsing
  existing ANSI-styled text.
  ([e14f645](https://github.com/Dicklesworthstone/rich_rust/commit/e14f6457b10f9d3c1f3af51fbfbf1c6afe3599c3))
- `rich.protocol` parity (`rich_cast` + measure).
  ([770c3ee](https://github.com/Dicklesworthstone/rich_rust/commit/770c3ee72f613ca02f7345fbe7035e3c9092ea26))
- Interactive key handling and start wizard for demo_showcase.
  ([da65d3c](https://github.com/Dicklesworthstone/rich_rust/commit/da65d3c2ea649f934ab016540f99e39c8a45bd9d),
  [d450a82](https://github.com/Dicklesworthstone/rich_rust/commit/d450a825aa93f5b161cdefc9771c0b01704f0870))
- Correct cursor positioning to match Python Rich's 0-based API.
  ([67cf0d1](https://github.com/Dicklesworthstone/rich_rust/commit/67cf0d1c7eb08a8b43c72531c2ac497389df3932))

### Demo Showcase ("Nebula Deploy")

Complete interactive demo binary (`demo_showcase`) with a scene-based
architecture exercising every renderable in a fictional deployment
narrative. 15 scenes: hero, dashboard, table, panel, tree, layout, JSON,
markdown, syntax, traceback, emoji/hyperlinks, debug tools, tracing,
export, and outro.

- Scene trait, registry, and runner main loop
  ([bd2ca08](https://github.com/Dicklesworthstone/rich_rust/commit/bd2ca0899d06dfc76cbdaaef3e6251dc2efaabb3))
- CLI scaffold with config, theming, seed, and non-interactive safety
  ([29327fe](https://github.com/Dicklesworthstone/rich_rust/commit/29327fedbae26eae9b2f44f46abe1907c88abd46))
- Dashboard scene with live pipeline simulation and streaming logs
  ([9df47c6](https://github.com/Dicklesworthstone/rich_rust/commit/9df47c677c2a6e4dd10f5dfa1a586515e107cfe9),
  [cec1187](https://github.com/Dicklesworthstone/rich_rust/commit/cec1187e2326688915b65481a8fb7f2bcd7eadb3))
- Typography and spacing patterns module
  ([7aa7aee](https://github.com/Dicklesworthstone/rich_rust/commit/7aa7aee402a9bdb75708b6b6f4f15ceb55eec441),
  [b2becf0](https://github.com/Dicklesworthstone/rich_rust/commit/b2becf0d623d9bfdbf915749beb0ceec75801c1e))
- Status spinner moments
  ([e561f05](https://github.com/Dicklesworthstone/rich_rust/commit/e561f05d19f7805193bbb0d15b1f429eacdac3a9))
- Snapshot test harness for visual regression of all demo scenes
  ([8a0367c](https://github.com/Dicklesworthstone/rich_rust/commit/8a0367c56e0dee1f1c9e0f789bd212ef1504e342))
- Non-TTY safety and smoke tests
  ([c252013](https://github.com/Dicklesworthstone/rich_rust/commit/c252013e9374d796c8a87ee5965facbca801012a),
  [c2e8c88](https://github.com/Dicklesworthstone/rich_rust/commit/c2e8c8848ecc994ab0f9b70d447fd638a761a761))
- Wide terminal centered rendering
  ([309037a](https://github.com/Dicklesworthstone/rich_rust/commit/309037ab73dc87c548764aade0cebac25ffba660),
  [592b228](https://github.com/Dicklesworthstone/rich_rust/commit/592b228bca9c1d5780b8432d8fca0adf9dd02a53))

### Conformance and Python Rich Parity

- Syntax and Markdown ANSI output aligned with Python Rich fixtures
  ([4a4ef20](https://github.com/Dicklesworthstone/rich_rust/commit/4a4ef20be3c9f8cfbda492afd93e0090764c3ebd),
  [7653034](https://github.com/Dicklesworthstone/rich_rust/commit/76530342aea178d4a40f3cc1539391d6ddbdea23))
- Traceback ANSI parity with Python Rich fixtures
  ([31db427](https://github.com/Dicklesworthstone/rich_rust/commit/31db427a54b05784adb4662d437204d87f265127))
- `rich.control` parity with conformance fixtures
  ([3fa016c](https://github.com/Dicklesworthstone/rich_rust/commit/3fa016c07934bfb1efa522063e3da02be6a88f23))
- Export parity and Syntax wrap improvements
  ([c30a3b3](https://github.com/Dicklesworthstone/rich_rust/commit/c30a3b3958cdddecf4376f24eba63ee8f0709e28))
- Live stdio redirect, emoji toggle, and traceback improvements
  ([1a8ca77](https://github.com/Dicklesworthstone/rich_rust/commit/1a8ca779bab6986052568ee2d07ea9741978ee18))
- Fenced code wrap and crop under padding matched to Rich behavior
  ([6f75aa5](https://github.com/Dicklesworthstone/rich_rust/commit/6f75aa56b3658e73c12cd0e945aa9d831efcb7e3))
- Multiple parity fixes with conformance fixtures
  ([b9d32f5](https://github.com/Dicklesworthstone/rich_rust/commit/b9d32f5ddd66c6ef969e202cdb5361ba399fd5ee))

### Bug Fixes

- Prevent wide-char overflow in narrow fixed-width table columns
  ([69afd89](https://github.com/Dicklesworthstone/rich_rust/commit/69afd89c8c985d260678a17ab183fd4b0c31a4bc))
- Prevent gutter overflow when Columns width is narrower than separator
  budget
  ([1419340](https://github.com/Dicklesworthstone/rich_rust/commit/14193406dc18e48ed8923cef68a14172df8568ba))
- Clamp interactive `max_length` zero to one
  ([45284ef](https://github.com/Dicklesworthstone/rich_rust/commit/45284efb49e3fa3c6cadad123ff87faa4dc3de5c))
- Preserve CRLF content in `LiveWriter` proxy
  ([f35f502](https://github.com/Dicklesworthstone/rich_rust/commit/f35f50293594d1e5a6bb78f7c07a9fbf78a362eb))
- Fix nested list indentation in Markdown rendering
  ([0d3b315](https://github.com/Dicklesworthstone/rich_rust/commit/0d3b3151d69b28757734fb933ac79260511c7a61))
- Handle `FORCE_COLOR=0` as "unset" per convention
  ([f4e3e5a](https://github.com/Dicklesworthstone/rich_rust/commit/f4e3e5a37ebe4ceb73512865fe66870296958aac))
- Prevent panic on malformed markup handler syntax
  ([4bfbaef](https://github.com/Dicklesworthstone/rich_rust/commit/4bfbaefe3db5ffef370a83a349939aa63189a8cd))
- Preserve Tree label spans during newline sanitization
  ([9a63bc9](https://github.com/Dicklesworthstone/rich_rust/commit/9a63bc9a658e96712b310112182339c0b04d0e7b))
- Properly handle nested structures in Inspect renderable
  ([870a893](https://github.com/Dicklesworthstone/rich_rust/commit/870a893a31c81b29221fe9194d4b987759d798ca),
  [1080a6b](https://github.com/Dicklesworthstone/rich_rust/commit/1080a6b6886e0c95769a08e05080557db1f9bb30))
- Preserve source indentation in Traceback and stop filtering "main" as
  internal frame
  ([540595e](https://github.com/Dicklesworthstone/rich_rust/commit/540595ee1e9631a3a0e3ea97e4f84a6794229a1b))
- Fix `time::Month` formatting for time 0.3 crate
  ([913f1ad](https://github.com/Dicklesworthstone/rich_rust/commit/913f1ad7a03bb706bd766c8aade9d34b4fe221b0))
- Fix timestamp formatting and ANSI test harness
  ([18e7528](https://github.com/Dicklesworthstone/rich_rust/commit/18e75283a127b14f81c267863c0de82b300f2b34))
- Fix hyperlinks display and title markup in emoji_links demo scene
  ([7661712](https://github.com/Dicklesworthstone/rich_rust/commit/766171245394970ff5d0f870f2ed1619d06110eb))
- Center hero text relative to console width
  ([6a4b43b](https://github.com/Dicklesworthstone/rich_rust/commit/6a4b43ba26e4421f0d5f7715e325199b965a90f8))
- Use consistent Rounded guides in tree scene
  ([a22da86](https://github.com/Dicklesworthstone/rich_rust/commit/a22da86fc326a85bff19573ba48046c218ba2242))

### Performance

- Use `SmallVec` for hot paths to reduce heap allocations
  ([6f293d4](https://github.com/Dicklesworthstone/rich_rust/commit/6f293d42c90ceaf2a1a86126c4c66804bb007d29))
- Comprehensive clippy pedantic lint cleanup across codebase
  ([4c27749](https://github.com/Dicklesworthstone/rich_rust/commit/4c27749dfb17589195a352e988d21c47a6764e0d),
  [c10af97](https://github.com/Dicklesworthstone/rich_rust/commit/c10af97b803b50c34e545f26c11efaccde692d88),
  [b60f651](https://github.com/Dicklesworthstone/rich_rust/commit/b60f651ebdd87c72e2356b7fa0cb3d2312c40859))
- Updated render benchmarks
  ([17e1860](https://github.com/Dicklesworthstone/rich_rust/commit/17e18607da59939d78a05edf77408bfeb6bcb17a))

### Testing Infrastructure

- Property-based testing with proptest
  ([9e52fa2](https://github.com/Dicklesworthstone/rich_rust/commit/9e52fa27506694eefd2f304f5aadcb8737b325ee))
- Flaky test detection and retry infrastructure
  ([e3114d3](https://github.com/Dicklesworthstone/rich_rust/commit/e3114d3dc20c9e8436467960d7d07003d97238e1))
- Serial test support for env var isolation
  ([b3f6b07](https://github.com/Dicklesworthstone/rich_rust/commit/b3f6b075d760f51c114481e2d915f04afae002ca))
- E2E test suites for wide terminals, logging, and markdown
  ([66c5fa9](https://github.com/Dicklesworthstone/rich_rust/commit/66c5fa94c071c79cad0c4c97833801c92d7bfc39),
  [534b0fb](https://github.com/Dicklesworthstone/rich_rust/commit/534b0fbaa397744e1c697b6e220090193a951d81))
- Coverage tracking and reporting infrastructure
  ([d0fbe2a](https://github.com/Dicklesworthstone/rich_rust/commit/d0fbe2abbf7267e9f513880f3710a2cf1708098c))
- Test naming standards and lint enforcement
  ([9514f5f](https://github.com/Dicklesworthstone/rich_rust/commit/9514f5f03c3e45d16b0648f726da2ff597c137ca))
- Platform-specific fixtures and validation utilities
  ([0a2ccaf](https://github.com/Dicklesworthstone/rich_rust/commit/0a2ccaf0b7d913c9b66b3576a15e21285c88020b))
- Comprehensive theme, emoji, table column width, and Group/Prompt unit
  tests
  ([2c99888](https://github.com/Dicklesworthstone/rich_rust/commit/2c99888790139494344678b5e9774cbef2bc26dd),
  [ab782a5](https://github.com/Dicklesworthstone/rich_rust/commit/ab782a5097f427f96d0a5bce3b2819dbf9d1c968),
  [1b048f1](https://github.com/Dicklesworthstone/rich_rust/commit/1b048f16bb59a0687e1b6b0f460083030081f031),
  [c9434ce](https://github.com/Dicklesworthstone/rich_rust/commit/c9434ceea7232981c0ff2b49fcb9d9d9b2fff6f6))

---

## [0.1.1] -- 2026-01-25

[GitHub Release](https://github.com/Dicklesworthstone/rich_rust/releases/tag/v0.1.1) · annotated tag · 33 commits since v0.1.0

Stabilization patch. Adds the Layout, Live display, and Logging systems;
fixes CI across platforms; and resolves rendering bugs in rules and
markdown.

### New Capabilities

- **Layout, Live display, and Logging** systems in a major feature parity
  push -- brings `Layout` for split-screen arrangements, `Live` for
  dynamic terminal refresh, and `RichLogger` for `log` crate integration.
  ([399e228](https://github.com/Dicklesworthstone/rich_rust/commit/399e2284cdc1d8093f1f44bfa69e578cbf727c8e))
- Conformance fixtures and layout fixes
  ([ccad15d](https://github.com/Dicklesworthstone/rich_rust/commit/ccad15d03991e626782887eff9f532775b7979c1))
- Progress tests module added to conformance suite
  ([1c1b01f](https://github.com/Dicklesworthstone/rich_rust/commit/1c1b01f3c0a80311bd9232d8d3e4962830817464))
- Honor `force_terminal` for color detection
  ([09e1a8c](https://github.com/Dicklesworthstone/rich_rust/commit/09e1a8cc2869d308972003fd383c36bf69a4324b))
- MIT License file added
  ([4885d89](https://github.com/Dicklesworthstone/rich_rust/commit/4885d8991160b1f82fd0aa0094b1871cb8447f67))

### Bug Fixes

- Fix `log` crate `std` feature and unicode-width compatibility (the tag
  commit)
  ([5d0c0cc](https://github.com/Dicklesworthstone/rich_rust/commit/5d0c0cc33d3424fef7800ed4e48c6312cfbc1ebc))
- Sanitize newlines in rule titles to prevent layout breakage
  ([f842e6a](https://github.com/Dicklesworthstone/rich_rust/commit/f842e6a2bbc38a5b8565f4a8004029e2d6e82570))
- Fix markdown list continuation indent
  ([be7a034](https://github.com/Dicklesworthstone/rich_rust/commit/be7a0344db58acc6ae697187c59df43827b2fd80))
- Make `PrintOptions.soft_wrap` actually enable text wrapping
  ([879862b](https://github.com/Dicklesworthstone/rich_rust/commit/879862baf2e9a91b09ebbfb9033348b95f63fafe))
- Remove `let_chains` nightly feature flag for broader compatibility
  ([dd3dcdb](https://github.com/Dicklesworthstone/rich_rust/commit/dd3dcdb80bff2b4f544f88ed3147dd28e8d5e7d7))
- Relax unicode-width version constraint
  ([f021b9f](https://github.com/Dicklesworthstone/rich_rust/commit/f021b9f6ca61ec9d0bcb4213f701d0dd45de00a8))

### CI Stabilization

- Use branch ref for `dtolnay/rust-toolchain` action
  ([aa98edc](https://github.com/Dicklesworthstone/rich_rust/commit/aa98edcd4570a87d14e7a6d0eef9473c1a729f07))
- Fix multiple workflow failures
  ([2fa9e00](https://github.com/Dicklesworthstone/rich_rust/commit/2fa9e002f293ddf2e5dd917b09f0b1371c33a827))
- Pin advisory DB and silence Windows unused param warnings
  ([c36ea83](https://github.com/Dicklesworthstone/rich_rust/commit/c36ea830dfc7bc8c371315330fedc5701a510049))
- Handle cargo-audit CVSS 4.0 issue and Windows test warnings
  ([f5d4c54](https://github.com/Dicklesworthstone/rich_rust/commit/f5d4c5405ff22c08a1394bf8391453f3a5b9c1d2))
- Resolve clippy warnings and update test snapshots
  ([9c15b22](https://github.com/Dicklesworthstone/rich_rust/commit/9c15b228d2dcd87029ed3b3b6de74a18870dc8e1))
- Remove invalid `--all-features` flag from coverage report
  ([a8bf3a5](https://github.com/Dicklesworthstone/rich_rust/commit/a8bf3a592c9338ed7c0fbbeddf74faf57131d092))

### Rendering Improvements

- Improve text rendering
  ([5dab327](https://github.com/Dicklesworthstone/rich_rust/commit/5dab3271dc61f0d385c2ee05069cf0ee0f12ac42))
- Improve console rendering and update documentation
  ([d278086](https://github.com/Dicklesworthstone/rich_rust/commit/d278086fd0f99c2a2bb76c2ca8a56a77c2370907))
- Enhance renderables module
  ([6d912ac](https://github.com/Dicklesworthstone/rich_rust/commit/6d912ac59407db344302e8dc947a3f7b44798ed9))

---

## [0.1.0] -- 2026-01-19

[GitHub Release](https://github.com/Dicklesworthstone/rich_rust/releases/tag/v0.1.0) · annotated tag · 141 commits · initial release

First public release of rich_rust: a Rust port of Python's Rich library
for beautiful terminal output. Approximately 11,800 lines of idiomatic,
`#![forbid(unsafe_code)]` Rust across 20+ modules. Published to
[crates.io](https://crates.io/crates/rich_rust).

### Core Engine

- **Console** coordinator with automatic terminal capability detection
  (4-bit, 8-bit, 24-bit truecolor), configurable output streams, and
  ANSI SGR generation.
  ([b05abdb](https://github.com/Dicklesworthstone/rich_rust/commit/b05abdbbe266fe84a12f020e998fde7c82c91e3c),
  [19b5187](https://github.com/Dicklesworthstone/rich_rust/commit/19b518759f81c0a615ea87c16dcde86603f10820))
- **Style** system with bold, italic, underline, strikethrough, dim,
  reverse, foreground/background colors, hyperlinks (`link_id`), and
  meta fields. LRU cache for `render_ansi()` output.
  ([d2ce02f](https://github.com/Dicklesworthstone/rich_rust/commit/d2ce02f2a040545333248a0061d945980d58beff),
  [c67e00a](https://github.com/Dicklesworthstone/rich_rust/commit/c67e00a6554388360a982dc58d8ce8d28dea9c07))
- **Text** with rich markup parsing (`[bold red]text[/]`), styled spans,
  and Unicode cell width handling.
- **Segment** as atomic rendering unit using `Cow<'a, str>` for
  zero-allocation paths.
  ([c4bf8fd](https://github.com/Dicklesworthstone/rich_rust/commit/c4bf8fd5de0b6886b12e3d18a097dafe227a1a32))
- **Color** system covering standard 16, 256-color palette, and 24-bit
  RGB/hex. Validated against RICH_SPEC Section 1.
  ([d3c14c8](https://github.com/Dicklesworthstone/rich_rust/commit/d3c14c80c234a6cd6e973f733b3b408e34ffd639))
- **Markup** parser for `[bold red]text[/]` style syntax.
- **Terminal** detection for dimensions, color support, Unicode
  capabilities, and legacy Windows console.
- **Renderable** trait for ergonomic `console.print_renderable()`
  dispatching; blanket impls for `str`/`String`.
  ([053d8aa](https://github.com/Dicklesworthstone/rich_rust/commit/053d8aa4ab2f7f756c4d46d0b939207a1ab3da3d),
  [ca4d119](https://github.com/Dicklesworthstone/rich_rust/commit/ca4d119afcfda526a2ab22343bc9525645b846f5))

### Renderables

- **Table** with auto-sizing columns, borders, headers/footers, leading
  support, vertical padding, and rounding correction in
  `collapse_widths()`.
  ([e160e4f](https://github.com/Dicklesworthstone/rich_rust/commit/e160e4f4080bc806be70daa0a3d3db8dc7988bbf),
  [021b890](https://github.com/Dicklesworthstone/rich_rust/commit/021b890d558d2ce4d00b1ff6b0f55b81a03b39da),
  [c3b5a36](https://github.com/Dicklesworthstone/rich_rust/commit/c3b5a36004485f9c9ad38f56b6a8326d83f3d68f))
- **Panel** with box styles (rounded, square, heavy, double, ASCII),
  titles, subtitles, and padding.
  ([883eae4](https://github.com/Dicklesworthstone/rich_rust/commit/883eae48c5669e3c3a728639ea35d57ae07c9a60))
- **Tree** with hierarchical data visualization and guide lines.
- **ProgressBar** with customizable progress tracking and spinners.
- **Rule** horizontal dividers with optional styled titles.
- **Columns** multi-column newspaper-style layout.
- **Padding** and **Align** layout helpers.

### Feature-gated Modules

- `syntax`: Syntax highlighting via syntect (100+ languages).
- `markdown`: Markdown rendering via pulldown-cmark (CommonMark + GFM).
- `json`: JSON pretty-printing with syntax highlighting.

### Performance Optimizations

- `Segment` uses `Cow<'a, str>` for zero-allocation rendering.
  ([c4bf8fd](https://github.com/Dicklesworthstone/rich_rust/commit/c4bf8fd5de0b6886b12e3d18a097dafe227a1a32))
- `Text::render` uses byte-index slicing to avoid intermediate `String`
  allocations.
  ([109a60e](https://github.com/Dicklesworthstone/rich_rust/commit/109a60e0c189ce4a381e0328cd376aa49c3ecdd1))
- `Style::render_ansi` returns `Arc`-cached strings to reduce cloning
  in the hot path.
  ([609a7bd](https://github.com/Dicklesworthstone/rich_rust/commit/609a7bd1a6a19eb9ce720c5ea22847ab091c25ba),
  [c67e00a](https://github.com/Dicklesworthstone/rich_rust/commit/c67e00a6554388360a982dc58d8ce8d28dea9c07))
- Ellipsis truncation fix for tiny widths.
  ([207946a](https://github.com/Dicklesworthstone/rich_rust/commit/207946aedfb1d26604ac13473b8c9f5fd24c17f9))
- `collapse_padding` width accounting fix.
  ([18b8286](https://github.com/Dicklesworthstone/rich_rust/commit/18b8286ff7f89b4ec6009e7773daf48ba0e4579a))
- Markdown multi-paragraph fix, rule truncation, `highlight_regex`
  optimization.
  ([7ac74ee](https://github.com/Dicklesworthstone/rich_rust/commit/7ac74eed7c713ca22aa1dacc888318ed49ee20ef))

### Validation and Specification

- RICH_SPEC.md validation complete for all sections: Color System,
  Padding, Unicode Cell Width, width algorithms, and more.
  ([53619cc](https://github.com/Dicklesworthstone/rich_rust/commit/53619cc4e5152362d85429fdbb41dd07c9b31324))
- Style parsing edge cases verified
  ([a54e3af](https://github.com/Dicklesworthstone/rich_rust/commit/a54e3aff2eb43bd372d8c77833859b3ca8a2bd2f))
- Hyperlink (OSC 8) support verified
  ([d0f8523](https://github.com/Dicklesworthstone/rich_rust/commit/d0f85235879b7150bc48af4ab6d3f64ec5ada970),
  [ca4bd56](https://github.com/Dicklesworthstone/rich_rust/commit/ca4bd5641308b1cd05cc47e69f4d43440d36bba2))
- Control character width calculation in `cell_len()` fixed
  ([1e85298](https://github.com/Dicklesworthstone/rich_rust/commit/1e852982b5787457c41874e106c1cd6aceeaa325))

### Testing and Infrastructure

- GitHub Actions CI/CD workflows
  ([46ae771](https://github.com/Dicklesworthstone/rich_rust/commit/46ae771681064f248dadbee27239cab6d27af217))
- Criterion benchmarks for rendering performance
  ([c67c126](https://github.com/Dicklesworthstone/rich_rust/commit/c67c126d10d1c929a82a4a4f462cf540181f68ad))
- Golden snapshot test infrastructure with insta
  ([5c7fe7d](https://github.com/Dicklesworthstone/rich_rust/commit/5c7fe7d5e718bf77e97b7ed33e42d1ea9756595c))
- E2E tests for rendering pipeline, Table, Panel, Text wrapping
  ([168eaba](https://github.com/Dicklesworthstone/rich_rust/commit/168eaba7fafabd0655445175c41bf9731ad9d7c5),
  [2394e6d](https://github.com/Dicklesworthstone/rich_rust/commit/2394e6dd1c4ce81f6937ec1eee6daf9f2c3313f9),
  [f1338cf](https://github.com/Dicklesworthstone/rich_rust/commit/f1338cf3f3e02de3be6bdf348b8b46aeefca3533))
- Property-based tests with proptest for invariant verification
  ([707740b](https://github.com/Dicklesworthstone/rich_rust/commit/707740b68d6592f9f3e3ccc715422d5dc4e67e41))
- Thread safety verified: `Console` is `Send` but not `Sync` (intentional)
  ([04e9087](https://github.com/Dicklesworthstone/rich_rust/commit/04e9087bdc97dbcf6e36ae31a1071bda290e88f8))
- Comprehensive Style and Color module unit tests achieving 100% coverage
  ([0c36f9f](https://github.com/Dicklesworthstone/rich_rust/commit/0c36f9fdc9b200c8cc955e530d4922c72e284161),
  [bf467c3](https://github.com/Dicklesworthstone/rich_rust/commit/bf467c3476290e345427eb5ca015ada281559eb3))
- Conformance and regression test suites
  ([0844d23](https://github.com/Dicklesworthstone/rich_rust/commit/0844d238e23668ace24924f1db45d163f3d6645b))
- Release workflow setup (macOS, Linux)
  ([8dd5b30](https://github.com/Dicklesworthstone/rich_rust/commit/8dd5b300cb5f60aa81e1af7fcfd445fc6b05cdaa),
  [38adcd4](https://github.com/Dicklesworthstone/rich_rust/commit/38adcd4b0ecfa62ff1e7fdb5f103cacdd9eac5db))

---

<!-- version comparison links -->
[Unreleased]: https://github.com/Dicklesworthstone/rich_rust/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Dicklesworthstone/rich_rust/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Dicklesworthstone/rich_rust/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Dicklesworthstone/rich_rust/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Dicklesworthstone/rich_rust/releases/tag/v0.1.0
