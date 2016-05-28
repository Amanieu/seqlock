SeqLock
=======

[![Build Status](https://travis-ci.org/Amanieu/seqlock.svg?branch=master)](https://travis-ci.org/Amanieu/seqlock) [![Crates.io](https://img.shields.io/crates/v/seqlock.svg)](https://crates.io/crates/seqlock)

[Documentation](https://amanieu.github.io/seqlock/seqlock/index.html)

This library provides the `SeqLock` type, which is a form of reader-writer
lock that is heavily optimized for readers.

In certain situations, `SeqLock` can be two orders of magnitude faster than
the standard library `RwLock` type. Another advantage is that readers cannot
starve writers: a writer will never block even if there are readers
currently accessing the `SeqLock`.

The only downside of `SeqLock` is that it only works on types that are
`Copy`. This means that it is unsuitable for types that contains pointers
to owned data.

You should instead use `RwLock` from the
[parking_lot](https://github.com/Amanieu/parking_lot) crate if you need
a reader-writer lock for types that are not `Copy`.

## Example

```
use seqlock::SeqLock;

let lock = SeqLock::new(5);

{
    // Writing to the data involves a lock
    let mut w = lock.lock_write();
    *w += 1;
    assert_eq!(*w, 6);
}

{
    // Reading the data is a very fast operation
    let r = lock.read();
    assert_eq!(r, 6);
}
```

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
seqlock = "0.1"
```

and this to your crate root:

```rust
extern crate seqlock;
```

To enable nightly-only features (currently just `const fn` constructors), add
this to your `Cargo.toml` instead:

```toml
[dependencies]
seqlock = {version = "0.1", features = ["nightly"]}
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
