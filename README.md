# Minimalistic DEFLATE Rust Library
![Rust Workflow](https://github.com/steschu77/miniz-rs/actions/workflows/ci.yml/badge.svg)

MiniZ-rs is a minimalistic (de-)compression library that provides support for decoding DEFLATE compressed data.

## Project Goals

MiniZ-rs is a minimalistic library that provides support for decoding DEFLATE compressed data. It is intended to be used in projects that require minimal dependencies and a small footprint.

## Features

* Decoding of DEFLATE compressed data
* Reading ZIP files (tbd.)
* Reading PNG files (tbd.)
* No dependencies

## Usage

Add the following to your `Cargo.toml`:

```toml
[dependencies]
miniz = { git = "https://github.com/steschu77/miniz-rs.git" }
```

In your Rust code, you can use the library like this:

```rust
use miniz::inflate::inflate;

fn main() {
    let mut out = [0u8; 262];
    inflate(&mut out, &[0x2b, 0x1f, 0x05, 0x40, 0x0c, 0x00]).unwrap();
    println!("{:?}", out);
}
```

## License

This project is licensed under the MIT license.
