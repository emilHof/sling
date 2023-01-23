This crates provides a sequentially locking Ring Buffer. It allows for
a fast and non-writer-blocking SPMC-queue, where all consumers read all
messages.

## Usage

There are two ways of consuming from the queue. If threads share a
`ReadGuard` through a shared reference, they will steal
queue items from one anothers such that no two threads will read the
same message. When a `ReadGuard` is cloned, the new
`ReadGuard`'s reading progress will no longer affect the other
one. If two threads each use a separate `ReadGuard`, they
will be able to read the same messages.

```rust
use sling::*;

let buffer = RingBuffer::<_, 256>::new();

let mut writer = buffer.try_lock().unwrap();
let mut reader = buffer.reader();

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

    for i in 0..100 {
        writer.push_back([i, i, i]);
    }
});
```

## Important!

It is also important to keep in mind, that slow readers will be overrun by the writer if they
do not consume messages quickly enough. This can happen quite frequently if the buffer size is
not large enough. It is advisable to test applications on a case-by-case basis and find a
buffer size that is optimal to your use-case.

## Benchmarks

Ping speeds compared to [Crossbeam-Channel's](https://crates.io/crates/crossbeam-channel) mpmc
channel and [lockfree's](https://crates.io/crates/lockfree) spmc channel.

![violin-plot](<./target/criterion/reports/Bench\ Sling\ Ping\ Variable\ Threads/violin.svg>)
