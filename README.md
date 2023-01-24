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

![violin-plot](<./target/criterion/reports/Bench Sling Ping Variable Threads/violin.svg>)
**Disclaimer**: These benchmarks were performed on a personal computer and may not be prepresentative
of relative performance on other computers. 

Specifications of the benchmarking system:
```
Architecture:            x86_64
  CPU op-mode(s):        32-bit, 64-bit
  Address sizes:         39 bits physical, 48 bits virtual
  Byte Order:            Little Endian
CPU(s):                  16
  On-line CPU(s) list:   0-15
Vendor ID:               GenuineIntel
  Model name:            Intel(R) Core(TM) i7-10700K CPU @ 3.80GHz
    CPU family:          6
    Model:               165
    Thread(s) per core:  2
    Core(s) per socket:  8
    Socket(s):           1
    Stepping:            5
    CPU(s) scaling MHz:  68%
    CPU max MHz:         5100.0000
    CPU min MHz:         800.0000
    BogoMIPS:            7602.45
    
Virtualization features:
  Virtualization:        VT-x
Caches (sum of all):
  L1d:                   256 KiB (8 instances)
  L1i:                   256 KiB (8 instances)
  L2:                    2 MiB (8 instances)
  L3:                    16 MiB (1 instance)
NUMA:
  NUMA node(s):          1
  NUMA node0 CPU(s):     0-15
  ```
