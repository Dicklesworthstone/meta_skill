Serde Regex
============

[Documentation](https://docs.rs/serde_regex) |
[Github](https://github.com/tailhook/serde-regex) |
[Crate](https://crates.io/crates/serde_regex)

A serde wrapper, that can be used to serialize regular expressions as strings.
It's often useful to read regexes from configuration file.

Note: regex is read with default settings. So DoS attack is probably possible
if reading regex from untrusted source. I.e. reading from config file is
okay, reading from API request is not.


Example
-------

```rust
#[macro_use]
extern crate serde_derive;

extern crate serde;
extern crate serde_regex;

use regex::Regex;

#[derive(Serialize, Deserialize)]
struct Timestamps {
    #[serde(with = "serde_regex")]
    pattern: Regex,
}
```


License
=======

Licensed under either of

* Apache License, Version 2.0,
  (./LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (./LICENSE-MIT or http://opensource.org/licenses/MIT)
  at your option.

Contribution
------------

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

