# stdio-override

[![Build Status](https://github.com/elichai/docs/stdio-override/workflows/ci.yml/badge.svg)](https://github.com/elichai/stdio-override/actions)
[![Latest version](https://img.shields.io/crates/v/stdio-override.svg)](https://crates.io/crates/stdio-override)
[![Documentation](https://docs.rs/stdio-override/badge.svg)](https://docs.rs/stdio-override)
![License](https://img.shields.io/crates/l/stdio-override.svg)

A Rust library to easily override Stdio streams in Rust. It works on Unix and Windows platforms.

- [Documentation](https://docs.rs/stdio-override)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
stdio-override = "0.1"
```

and for Rust Edition 2015 add this to your crate root:

```rust
extern crate stdio_override;
```

In Rust Edition 2018 you can simply do:

```rust
use stdio_override::*;
```

Here's an example on how to write stdout into a file:

```rust
use std::{fs::{read_to_string, remove_file}, io};
use stdio_override::StdoutOverride;

fn main() -> io::Result<()> {
    let file_name = "./readme_test.txt";

    let guard = StdoutOverride::from_file(file_name)?;
    println!("some output");
    drop(guard);

    let contents = read_to_string(file_name)?;
    assert_eq!("some output\n", contents);
    println!("Outside!");
    remove_file(file_name)?;
    Ok(())
}
```

On Unix(Linux/MacOS etc.) you can also do the same with sockets:

```rust

use std::{
    io::Read,
    net::{TcpListener, TcpStream},
};
use stdio_override::StdoutOverride;

#[cfg(unix)]
fn main() {
    let address = ("127.0.0.1", 5543);

    let listener = TcpListener::bind(address).unwrap();
    let socket = TcpStream::connect(address).unwrap();

    let guard = StdoutOverride::from_io(socket).unwrap();
    println!("12345");
    drop(guard);

    let mut contents = String::new();
    let (mut stream, _) = listener.accept().unwrap();
    stream.read_to_string(&mut contents).unwrap();

    assert_eq!("12345\n", contents);

    println!("Outside!");
}
#[cfg(not(unix))]
fn main() {}
```

Both will work the same for `Stderr` and if you want to input `Stdin` from a file/socket you can do the following:

```rust
use std::{fs::File, io::{self, Write}};
use stdio_override::StdinOverride;

fn main() -> io::Result<()> {
    let file_name = "./test_inputs.txt";

    {
        let mut file = File::create(&file_name)?;
        file.write_all(b"Data")?;
    }

    let guard = StdinOverride::from_file(file_name)?;

    let mut inputs = String::new();
    io::stdin().read_line(&mut inputs)?;

    drop(guard);

    assert_eq!("Data", inputs);
    // Stdin is working as usual again, because the guard is dropped.
    Ok(())
}
```
