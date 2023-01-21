//! This crates provides a sequentially locking Ring Buffer. It allows for
//! a fast and non-writer-blocking SPMC-queue, where all consumers read all
//! messages.
//!
//! # Usage
//!
//! There are two ways of consuming from the queue. If threads share a
//! [ReadGuard](ReadGuard) through a shared reference, they will steal
//! queue items from one anothers such that no two threads will read the
//! same message. When a [ReadGuard](ReadGuard) is cloned, the new
//! [ReadGuard](ReadGuard)'s reading progress will no longer affect the other
//! one. If two threads each use a separate [ReadGuard](ReadGuard), they
//! will be able to read the same messages.
//!
//! ```rust
//! # use sling::*;
//!
//! let buffer = RingBuffer::<_, 256>::new();
//!
//! let mut writer = buffer.try_lock().unwrap();
//! let mut reader = buffer.reader();
//!
//! std::thread::scope(|s| {
//!     let reader = &reader;
//!     for t in 0..8 {
//!         s.spawn(move || {
//!             for _ in 0..100 {
//!                 if let Some(val) = reader.pop_front() {
//!                     println!("t: {}, val: {:?}", t, val);
//!                 };
//!             }
//!         });
//!     }
//!
//!     for i in 0..100 {
//!         writer.push_back([i, i, i]);
//!     }
//! });
//! ```
//! # Important!
//!
//! It is also important to keep in mind, that slow readers will be overrun by the writer if they
//! do not consume messages quickly enough. This can happen quite frequently if the buffer size is
//! not large enough. It is advisable to test applications on a case-by-case basis and find a
//! buffer size that is optimal to your use-case.
//!

#![warn(missing_docs)]
#![cfg_attr(feature = "nightly", feature(const_ptr_write))]
#![cfg_attr(feature = "nightly", feature(const_mut_refs))]
#![cfg_attr(feature = "nightly", feature(const_ptr_read))]
#![cfg_attr(feature = "nightly", feature(const_refs_to_cell))]

use core::cell::UnsafeCell;
use core::fmt::{Debug, Display};
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::ptr::{read_volatile, write_bytes, write_volatile};
use core::sync::atomic::{fence, AtomicBool, AtomicUsize, Ordering};

/// A fixed-size, non-write-blocking, ring buffer, that behaves like a
/// SPMC queue and can be safely shared across threads.
/// It is limited to only work for types that are copy, as multiple
/// threads can read the same message.
#[derive(Debug)]
pub struct RingBuffer<T: Copy, const N: usize> {
    // what else goes here?
    // version?
    // TODO(Emil): Can we make sure this is properly aligned for cache loads?
    locked: AtomicBool,
    version: AtomicUsize,
    index: AtomicUsize,
    data: [Block<T>; N],
}

unsafe impl<T: Copy, const N: usize> Send for RingBuffer<T, N> {}
unsafe impl<T: Copy, const N: usize> Sync for RingBuffer<T, N> {}

impl<T: Copy, const N: usize> RingBuffer<T, N> {
    /// Const constructor that only works on nightly with this crates `nightly` feature
    /// enabled. Constructs an empty queue of fixed length.
    #[cfg(feature = "nightly")]
    pub const fn new() -> RingBuffer<T, N> {
        // Initialize the array.
        let data: [Block<T>; N] = unsafe {
            let mut data: [MaybeUninit<Block<T>>; N] = MaybeUninit::uninit().assume_init();
            write_bytes(&mut data, 0, 1);

            // This workaround is currently necessary, as `core::mem::transmute()` is not available
            // for arrays whose length is specified by Const Generics.
            let init = core::ptr::read(
                (&data as *const [MaybeUninit<Block<T>>; N]).cast::<[Block<T>; N]>(),
            );
            core::mem::forget(data);
            init
        };

        RingBuffer {
            locked: AtomicBool::new(false),
            version: AtomicUsize::new(0),
            index: AtomicUsize::new(0),
            data,
        }
    }

    /// Constructs a new, empty array with a fixed length.
    /// ```rust
    /// # use sling::*;
    /// let buffer: RingBuffer<[u8; 16], 1024> = RingBuffer::new();
    /// ```
    #[cfg(not(feature = "nightly"))]
    pub fn new() -> RingBuffer<T, N> {
        // Initialize the array.
        let data: [Block<T>; N] = unsafe {
            let mut data: [MaybeUninit<Block<T>>; N] = MaybeUninit::uninit().assume_init();
            write_bytes(&mut data, 0, 1);

            // This workaround is currently necessary, as `core::mem::transmute()` is not available
            // for arrays whose length is specified by Const Generics.
            let init = core::ptr::read(
                (&data as *const [MaybeUninit<Block<T>>; N]).cast::<[Block<T>; N]>(),
            );
            core::mem::forget(data);
            init
        };

        RingBuffer {
            locked: AtomicBool::new(false),
            version: AtomicUsize::new(0),
            index: AtomicUsize::new(0),
            data,
        }
    }

    /// Tries to acquire the [RingBuffer's](RingBuffer) [WriteGuard](WriteGuard). As there can
    /// only ever be one thread holding a [WriteGuard](WriteGuard), this fails if another thread is
    /// already holding the lock.
    /// ```rust
    /// # use sling::*;
    /// let buffer: RingBuffer<[u8; 16], 1024> = RingBuffer::new();
    ///
    /// let Ok(mut writer) = buffer.try_lock() else { return };
    /// ```
    #[inline]
    pub fn try_lock(&self) -> Result<WriteGuard<'_, T, N>, ()> {
        if !self.locked.swap(true, Ordering::Acquire) {
            Ok(WriteGuard { buffer: &self })
        } else {
            Err(())
        }
    }

    /// Creates a new [ReadGuard](ReadGuard) which provides shared read access of the queue. The
    /// progress of this [ReadGuard](ReadGuard) is not affected by other [ReadGuards](ReadGuard)
    /// and does not affect them in turn.
    /// ```rust
    /// # use sling::*;
    /// let buffer: RingBuffer<[u8; 16], 1024> = RingBuffer::new();
    ///
    /// let reader = buffer.reader();
    /// ```
    #[inline]
    pub fn reader(&self) -> ReadGuard<'_, T, N> {
        ReadGuard {
            buffer: self,
            index: AtomicUsize::new(0),
            version: AtomicUsize::new(self.version.load(Ordering::Acquire)),
        }
    }

    /// Increments the sequence at the current index by 1, making it odd, prohibiting reads.
    #[inline]
    fn start_write(&self) -> usize {
        let index = self.index.load(Ordering::Relaxed);
        let seq = self.data[index].seq.fetch_add(1, Ordering::Relaxed);

        // Make sure the state is consistent.
        assert!(seq % 2 == 0);

        // Update the global version to be at newer than the current block version.
        self.version.fetch_max(seq + 2, Ordering::Release);

        fence(Ordering::Release);

        index
    }

    /// Increments the sequence at the current index by 1, making it even and allowing reads.
    #[inline]
    fn end_write(&self, index: usize) {
        self.index.store((index + 1) % N, Ordering::Relaxed);
        let seq = self.data[index].seq.fetch_add(1, Ordering::Release);

        // Ensure a consistent state.
        assert!(seq % 2 == 1);
    }
}

/// Shared read access to its buffer. When multiple threads consume from the
/// [RingBuffer](RingBuffer) throught the same [ReadGuard](ReadGuard), they will share progress
/// on the queue. Distinct [RingBuffers](RingBuffer) do not share progress.
#[derive(Debug)]
pub struct ReadGuard<'read, T: Copy, const N: usize> {
    buffer: &'read RingBuffer<T, N>,
    index: AtomicUsize,
    version: AtomicUsize,
}

/// Clones a [RingBuffer](RingBuffer), creating a new one that does not share progress with the
/// original [RingBuffer](RingBuffer).
impl<'read, T: Copy, const N: usize> Clone for ReadGuard<'read, T, N> {
    fn clone(&self) -> Self {
        ReadGuard {
            buffer: self.buffer,
            index: AtomicUsize::new(self.index.load(Ordering::Acquire)),
            version: AtomicUsize::new(self.version.load(Ordering::Relaxed)),
        }
    }
}

unsafe impl<'read, T: Copy, const N: usize> Send for ReadGuard<'read, T, N> {}

impl<'read, T: Copy, const N: usize> ReadGuard<'read, T, N> {
    /// Pops the next element from the front. The element is only popped for us and other threads
    /// will still need to pop this for themselves.
    pub fn pop_front(&self) -> Option<T> {
        // Checks if data if we are currently caught up.
        let i = self.check_version()?;

        loop {
            let seq1 = self.buffer.data[i].seq.load(Ordering::Acquire);

            if seq1 & 1 != 0 {
                // Spin until we messages is written.
                core::hint::spin_loop();
                continue;
            }

            // # Safety: We ensure validity of the read with the equality check later.
            let data: T = unsafe { read_volatile(self.buffer.data[i].message.get().cast()) };

            fence(Ordering::Acquire);

            let seq2 = self.buffer.data[i].seq.load(Ordering::Acquire);

            if seq1 == seq2 {
                return Some(data);
            }
        }
    }

    /// Checks if we are reading data we have already consumed.
    #[inline]
    fn check_version(&self) -> Option<usize> {
        // The current version of the
        let mut i = self.index.load(Ordering::Acquire);

        loop {
            let ver = self.version.load(Ordering::Relaxed);
            let seq = self.buffer.data[i].seq.load(Ordering::Acquire) & (usize::MAX - 1);

            // If we are in the beginning of the array and the version is current, that means
            if (i == 0 && seq == ver) || seq < ver {
                return None;
            }

            self.version.fetch_max(seq, Ordering::Relaxed);

            match self
                .index
                .compare_exchange(i, (i + 1) % N, Ordering::Release, Ordering::Acquire)
            {
                Err(new) => i = new,
                Ok(i) => return Some(i),
            }
        }
    }
}

/// Provides exclusive write access to the [RingBuffer](RingBuffer).
#[derive(Debug)]
pub struct WriteGuard<'write, T: Copy, const N: usize> {
    buffer: &'write RingBuffer<T, N>,
}

unsafe impl<'read, T: Copy, const N: usize> Send for WriteGuard<'read, T, N> {}

impl<'write, T: Copy, const N: usize> WriteGuard<'write, T, N> {
    /// Push a new value to the back of the queue. This operation does not block.
    /// ```rust
    /// # use sling::*;
    /// let buffer: RingBuffer<[u8; 3], 1024> = RingBuffer::new();
    ///
    /// if let Ok(mut writer) = buffer.try_lock() {
    ///     writer.push_back([12, 21, 04])
    /// };
    /// ```
    pub fn push_back(&mut self, val: T) {
        let index = self.buffer.start_write();
        unsafe { write_volatile(self.buffer.data[index].message.get().cast(), val) };
        self.buffer.end_write(index);
    }
}

impl<'write, T: Copy, const N: usize> Drop for WriteGuard<'write, T, N> {
    fn drop(&mut self) {
        self.buffer.locked.store(false, Ordering::Release);
    }
}

#[derive(Debug)]
struct Block<T: Copy> {
    seq: AtomicUsize,
    message: UnsafeCell<MaybeUninit<T>>,
}

/// Aligns its contents to the cache line.
#[cfg_attr(
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
    ),
    repr(align(128))
)]
#[cfg_attr(
    any(
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "riscv64",
    ),
    repr(align(32))
)]
#[cfg_attr(target_arch = "s390x", repr(align(256)))]
#[cfg_attr(
    not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "arm",
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "riscv64",
        target_arch = "s390x",
    )),
    repr(align(64))
)]
#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
struct Padded<T>(T);

impl<T> Padded<T> {
    const fn new(t: T) -> Self {
        Padded(t)
    }

    fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Padded<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Padded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Debug for Padded<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self.0))
    }
}

impl<T> Display for Padded<T>
where
    T: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.0))
    }
}

impl<T> From<T> for Padded<T> {
    fn from(value: T) -> Self {
        Padded::new(value)
    }
}

mod test {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_nightly() {
        if cfg!(feature = "nightly") {
            println!("nightly")
        }
    }

    #[test]
    fn test_new_buffer() {
        let _ = RingBuffer::<u32, 32>::new();
    }

    #[test]
    fn test_write() {
        let buffer = RingBuffer::<_, 32>::new();

        let mut writer = buffer.try_lock().unwrap();

        for i in 0..32 {
            writer.push_back(i)
        }

        println!("buffer: {:?}", buffer);
    }

    #[test]
    fn test_empty_queue() {
        let buffer = RingBuffer::<u8, 32>::new();

        let reader = buffer.reader();

        assert!(reader.pop_front().is_none());
    }

    #[test]
    fn test_lock() {
        let buffer = RingBuffer::<(), 32>::new();

        let _writer = buffer.try_lock().unwrap();

        assert!(buffer.try_lock().is_err());
    }

    #[test]
    fn test_read() {
        let buffer = RingBuffer::<_, 32>::new();

        let mut writer = buffer.try_lock().unwrap();

        for i in 0..32 {
            writer.push_back(i)
        }

        let reader = buffer.reader();

        while let Some(i) = reader.pop_front() {
            println!("val: {i}");
        }
    }

    #[test]
    fn test_multi_reader() {
        let buffer = RingBuffer::<_, 128>::new();

        let mut writer = buffer.try_lock().unwrap();
        let reader = buffer.reader();

        std::thread::scope(|s| {
            let reader = &reader;
            for t in 0..8 {
                s.spawn(move || {
                    for _ in 0..100 {
                        if let Some(val) = reader.pop_front() {
                            println!("t: {}, val: {:?}", t, val);
                        };
                    }
                });
            }

            for _ in 0..100 {
                writer.push_back([0, 32, 31, 903, 1, 4, 23, 12, 4, 21]);
            }
        });
    }
}
