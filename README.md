SeqLock
=======

[![Crates.io](https://img.shields.io/crates/v/seqlock.svg)](https://crates.io/crates/seqlock)

[Documentation](https://docs.rs/seqlock/latest/seqlock/)

This library provides the `SeqLock` type, which is a form of reader-writer
lock that is heavily optimized for readers.

In certain situations, `SeqLock` can be two orders of magnitude faster than
the standard library `RwLock` type. Another advantage is that readers cannot
starve writers: a writer will never block even if there are readers
currently accessing the `SeqLock`.

The only downside of `SeqLock` is that it only works on types that are
`Copy`. This means that it is unsuitable for types that contains pointers
to owned data.

You should instead use a `RwLock` if you need
a reader-writer lock for types that are not `Copy`.

## Implementation

The implementation is based on the [Linux `seqlock` type](http://lxr.free-electrons.com/source/include/linux/seqlock.h).
Each `SeqLock` contains a sequence counter which tracks modifications to the
locked data. The basic idea is that a reader will perform the following
operations:

1. Read the sequence counter.
2. Read the data in the lock.
3. Read the sequence counter again.
4. If the two sequence counter values differ, loop back and try again.
5. Otherwise return the data that was read in step 2.

Similarly, a writer will increment the sequence counter before and after
writing to the data in the lock. Once a reader sees that the sequence
counter has not changed while it was reading the data, it can safely return
that data to the caller since it is known to be in a consistent state.

## Example

```rust
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

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
