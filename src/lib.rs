//! This library provides the `SeqLock` type, which is a form of reader-writer
//! lock that is heavily optimized for readers.
//!
//! In certain situations, `SeqLock` can be two orders of magnitude faster than
//! the standard library `RwLock` type. Another advantage is that readers cannot
//! starve writers: a writer will never block even if there are readers
//! currently accessing the `SeqLock`.
//!
//! The only downside of `SeqLock` is that it only works on types that are
//! `Copy`. This means that it is unsuitable for types that contains pointers
//! to owned data.
//!
//! You should instead use a `RwLock` if you need
//! a reader-writer lock for types that are not `Copy`.
//!
//! # Implementation
//!
//! The implementation is based on the [Linux seqlock type](http://lxr.free-electrons.com/source/include/linux/seqlock.h).
//! Each `SeqLock` contains a sequence counter which tracks modifications to the
//! locked data. The basic idea is that a reader will perform the following
//! operations:
//!
//! 1. Read the sequence counter.
//! 2. Read the data in the lock.
//! 3. Read the sequence counter again.
//! 4. If the two sequence counter values differ, loop back and try again.
//! 5. Otherwise return the data that was read in step 2.
//!
//! Similarly, a writer will increment the sequence counter before and after
//! writing to the data in the lock. Once a reader sees that the sequence
//! counter has not changed while it was reading the data, it can safely return
//! that data to the caller since it is known to be in a consistent state.
//!
//! # Examples
//!
//! ```
//! use seqlock::SeqLock;
//!
//! let lock = SeqLock::new(5);
//!
//! {
//!     // Writing to the data involves a lock
//!     let mut w = lock.lock_write();
//!     *w += 1;
//!     assert_eq!(*w, 6);
//! }
//!
//! {
//!     // Reading the data is a very fast operation
//!     let r = lock.read();
//!     assert_eq!(r, 6);
//! }
//! ```

#![warn(missing_docs, rust_2018_idioms)]

use parking_lot::{Mutex, MutexGuard};
use std::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{fence, AtomicUsize, Ordering};
use std::thread;

/// A sequential lock
pub struct SeqLock<T> {
    seq: AtomicUsize,
    data: UnsafeCell<T>,
    mutex: Mutex<()>,
}

unsafe impl<T: Send> Send for SeqLock<T> {}
unsafe impl<T: Send> Sync for SeqLock<T> {}

/// RAII structure used to release the exclusive write access of a `SeqLock`
/// when dropped.
pub struct SeqLockGuard<'a, T> {
    _guard: MutexGuard<'a, ()>,
    seqlock: &'a SeqLock<T>,
    seq: usize,
}

impl<T> SeqLock<T> {
    #[inline]
    fn end_write(&self, seq: usize) {
        // Increment the sequence number again, which will make it even and
        // allow readers to access the data. The release ordering ensures that
        // all writes to the data are done before writing the sequence number.
        self.seq.store(seq.wrapping_add(1), Ordering::Release);
    }
}

impl<T: Copy> SeqLock<T> {
    /// Creates a new SeqLock with the given initial value.
    #[inline]
    pub const fn new(val: T) -> SeqLock<T> {
        SeqLock {
            seq: AtomicUsize::new(0),
            data: UnsafeCell::new(val),
            mutex: Mutex::new(()),
        }
    }

    /// Reads the value protected by the `SeqLock`.
    ///
    /// This operation is extremely fast since it only reads the `SeqLock`,
    /// which allows multiple readers to read the value without interfering with
    /// each other.
    ///
    /// If a writer is currently modifying the contained value then the calling
    /// thread will block until the writer thread releases the lock.
    ///
    /// Attempting to read from a `SeqLock` while already holding a write lock
    /// in the current thread will result in a deadlock.
    #[inline]
    pub fn read(&self) -> T {
        loop {
            // Load the first sequence number. The acquire ordering ensures that
            // this is done before reading the data.
            let seq1 = self.seq.load(Ordering::Acquire);

            // If the sequence number is odd then it means a writer is currently
            // modifying the value.
            if seq1 & 1 != 0 {
                // Yield to give the writer a chance to finish. Writing is
                // expected to be relatively rare anyways so this isn't too
                // performance critical.
                thread::yield_now();
                continue;
            }

            // We need to use a volatile read here because the data may be
            // concurrently modified by a writer. We also use MaybeUninit in
            // case we read the data in the middle of a modification.
            let result = unsafe { ptr::read_volatile(self.data.get() as *mut MaybeUninit<T>) };

            // Make sure the seq2 read occurs after reading the data. What we
            // ideally want is a load(Release), but the Release ordering is not
            // available on loads.
            fence(Ordering::Acquire);

            // If the sequence number is the same then the data wasn't modified
            // while we were reading it, and can be returned.
            let seq2 = self.seq.load(Ordering::Relaxed);
            if seq1 == seq2 {
                return unsafe { result.assume_init() };
            }
        }
    }

    #[inline]
    fn begin_write(&self) -> usize {
        // Increment the sequence number. At this point, the number will be odd,
        // which will force readers to spin until we finish writing.
        let seq = self.seq.load(Ordering::Relaxed).wrapping_add(1);
        self.seq.store(seq, Ordering::Relaxed);

        // Make sure any writes to the data happen after incrementing the
        // sequence number. What we ideally want is a store(Acquire), but the
        // Acquire ordering is not available on stores.
        fence(Ordering::Release);

        seq
    }

    #[inline]
    fn lock_guard<'a>(&'a self, guard: MutexGuard<'a, ()>) -> SeqLockGuard<'a, T> {
        let seq = self.begin_write();
        SeqLockGuard {
            _guard: guard,
            seqlock: self,
            seq: seq,
        }
    }

    /// Locks this `SeqLock` with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function does not block while waiting for concurrent readers.
    /// Instead, readers will detect the concurrent write and retry the read.
    ///
    /// Returns an RAII guard which will drop the write access of this `SeqLock`
    /// when dropped.
    #[inline]
    pub fn lock_write(&self) -> SeqLockGuard<'_, T> {
        self.lock_guard(self.mutex.lock())
    }

    /// Attempts to lock this `SeqLock` with exclusive write access.
    ///
    /// If the lock could not be acquired at this time, then `None` is returned.
    /// Otherwise, an RAII guard is returned which will release the lock when
    /// it is dropped.
    ///
    /// This function does not block.
    #[inline]
    pub fn try_lock_write(&self) -> Option<SeqLockGuard<'_, T>> {
        self.mutex.try_lock().map(|g| self.lock_guard(g))
    }

    /// Consumes this `SeqLock`, returning the underlying data.
    #[inline]
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `SeqLock` mutably, no actual locking needs
    /// to take place---the mutable borrow statically guarantees no locks exist.
    #[inline]
    pub fn get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

impl<T: Copy + Default> Default for SeqLock<T> {
    #[inline]
    fn default() -> SeqLock<T> {
        SeqLock::new(Default::default())
    }
}

impl<T: Copy + fmt::Debug> fmt::Debug for SeqLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SeqLock {{ data: {:?} }}", &self.read())
    }
}

impl<'a, T: Copy + 'a> Deref for SeqLockGuard<'a, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.seqlock.data.get() }
    }
}

impl<'a, T: Copy + 'a> DerefMut for SeqLockGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.seqlock.data.get() }
    }
}

impl<T> Drop for SeqLockGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        self.seqlock.end_write(self.seq);
    }
}
