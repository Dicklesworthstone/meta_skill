<h1 align="center">path-tree</h1>

<div align="center">
  <p><strong>A lightweight high performance HTTP request router for Rust</strong></p>
</div>

<div align="center">
  <!-- Safety docs -->
  <a href="/">
    <img src="https://img.shields.io/badge/-safety!-success?style=flat-square" alt="Safety!" /></a>
  <!-- Docs.rs docs -->
  <a href="https://docs.rs/path-tree">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="Docs.rs docs" /></a>
  <!-- Crates version -->
  <a href="https://crates.io/crates/path-tree">
    <img src="https://img.shields.io/crates/v/path-tree.svg?style=flat-square"
    alt="Crates.io version" /></a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/path-tree">
    <img src="https://img.shields.io/crates/d/path-tree.svg?style=flat-square"
      alt="Download" /></a>
</div>

## Parameters Syntax

| Pattern                         | Kind                | Description                                                                    |
| ------------------------------- | ------------------- | ------------------------------------------------------------------------------ |
| `:name`                         | `Normal`            | Matches a path piece, excludes `/`                                             |
| `:name?`                        | `Optional`          | Matches an optional path piece, excludes `/`                                   |
| `/:name?/` `/:name?`            | `OptionalSegment`   | Matches an optional path segment, excludes `/`, prefix or suffix should be `/` |
| `+` `:name+`                    | `OneOrMore`         | Matches a path piece, includes `/`                                             |
| `*` `:name*`                    | `ZeroOrMore`        | Matches an optional path piece, includes `/`                                   |
| `/*/` `/*` `/:name*/` `/:name*` | `ZeroOrMoreSegment` | Matches zero or more path segments, prefix or suffix should be `/`             |

## Supports

| Case                    | Parameters  |
| ----------------------- | ----------- |
| `:a:b`                  | `a` `b`     |
| `:a:b?`                 | `a` `b`     |
| `:a-:b` `:a.:b` `:a~:b` | `a` `b`     |
| `:a_a-:b_b`             | `a_a` `b_b` |
| `:a\\:` `:a\\_`         | `a`         |
| `:a\\::b` `:a\\_:b`     | `a` `b`     |
| `:a*`                   | `a`         |
| `*`                     | `*1`        |
| `*.*`                   | `*1` `*2`   |
| `:a+`                   | `a`         |
| `+`                     | `+1`        |
| `+.+`                   | `+1` `+2`   |
| `/*/abc/+/def/g`        | `*1` `+2`   |

## Examples

```rust
use path_tree::PathTree;

/*
/ •0
├── api/
│   └── + •13
├── login •1
├── public/
│   └── ** •7
├── s
│   ├── ettings •3
│   │   └── /
│   │       └── : •4
│   └── ignup •2
└── : •5
    └── /
        └── : •6
            └── /
                ├── actions/
                │   └── :
                │       └── \:
                │           └── : •10
                ├── releases/download/
                │   └── :
                │       └── /
                │           └── :
                │               └── .
                │                   └── : •8
                ├── tags/
                │   └── :
                │       └── -
                │           └── :
                │               └── -
                │                   └── : •9
                ├── : •11
                └── ** •12
*/
let mut tree = PathTree::new();

tree.insert("/", 0);
tree.insert("/login", 1);
tree.insert("/signup", 2);
tree.insert("/settings", 3);
tree.insert("/settings/:page", 4);
tree.insert("/:user", 5);
tree.insert("/:user/:repo", 6);
tree.insert("/public/:any*", 7);
tree.insert("/:org/:repo/releases/download/:tag/:filename.:ext", 8);
tree.insert("/:org/:repo/tags/:day-:month-:year", 9);
tree.insert("/:org/:repo/actions/:name\\::verb", 10);
tree.insert("/:org/:repo/:page", 11);
tree.insert("/:org/:repo/*", 12);
tree.insert("/api/+", 13);

let (h, p) = tree.find("/").unwrap();
assert_eq!(h, &0);
assert_eq!(p.params(), vec![]);

let (h, p) = tree.find("/login").unwrap();
assert_eq!(h, &1);
assert_eq!(p.params(), vec![]);

let (h, p) = tree.find("/settings/admin").unwrap();
assert_eq!(h, &4);
assert_eq!(p.params(), vec![("page", "admin")]);

let (h, p) = tree.find("/viz-rs").unwrap();
assert_eq!(h, &5);
assert_eq!(p.params(), vec![("user", "viz-rs")]);

let (h, p) = tree.find("/viz-rs/path-tree").unwrap();
assert_eq!(h, &6);
assert_eq!(p.params(), vec![("user", "viz-rs"), ("repo", "path-tree")]);

let (h, p) = tree.find("/rust-lang/rust-analyzer/releases/download/2022-09-12/rust-analyzer-aarch64-apple-darwin.gz").unwrap();
assert_eq!(h, &8);
assert_eq!(
    p.params(),
    vec![
        ("org", "rust-lang"),
        ("repo", "rust-analyzer"),
        ("tag", "2022-09-12"),
        ("filename", "rust-analyzer-aarch64-apple-darwin"),
        ("ext", "gz")
    ]
);

let (h, p) = tree.find("/rust-lang/rust-analyzer/tags/2022-09-12").unwrap();
assert_eq!(h, &9);
assert_eq!(
    p.params(),
    vec![
        ("org", "rust-lang"),
        ("repo", "rust-analyzer"),
        ("day", "2022"),
        ("month", "09"),
        ("year", "12")
    ]
);

let (h, p) = tree.find("/rust-lang/rust-analyzer/actions/ci:bench").unwrap();
assert_eq!(h, &10);
assert_eq!(
    p.params(),
    vec![
        ("org", "rust-lang"),
        ("repo", "rust-analyzer"),
        ("name", "ci"),
        ("verb", "bench"),
    ]
);

let (h, p) = tree.find("/rust-lang/rust-analyzer/stargazers").unwrap();
assert_eq!(h, &11);
assert_eq!(p.params(), vec![("org", "rust-lang"), ("repo", "rust-analyzer"), ("page", "stargazers")]);

let (h, p) = tree.find("/rust-lang/rust-analyzer/stargazers/404").unwrap();
assert_eq!(h, &12);
assert_eq!(p.params(), vec![("org", "rust-lang"), ("repo", "rust-analyzer"), ("*1", "stargazers/404")]);

let (h, p) = tree.find("/public/js/main.js").unwrap();
assert_eq!(h, &7);
assert_eq!(p.params(), vec![("any", "js/main.js")]);

let (h, p) = tree.find("/api/v1").unwrap();
assert_eq!(h, &13);
assert_eq!(p.params(), vec![("+1", "v1")]);
```

Hyper hello example can be found [here](examples/hello.rs).

## Benchmark

```shell
$ cargo bench
```

## Acknowledgements

It is inspired by the:

- [rax]
- [httprouter]
- [echo] router
- [path-to-regexp]
- [gofiber] router
- [trekjs] router

## Other languages

Wrappers for path-tree in other languages:

- Python: https://github.com/adriangb/routrie

## License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>

[radix tree]: https://github.com/viz-rs/radix-tree
[rax]: https://github.com/antirez/rax
[httprouter]: https://github.com/julienschmidt/httprouter
[path-to-regexp]: https://github.com/pillarjs/path-to-regexp
[echo]: https://github.com/labstack/echo
[gofiber]: https://github.com/gofiber/fiber
[trekjs]: https://github.com/trekjs/router
