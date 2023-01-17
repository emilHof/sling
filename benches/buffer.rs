use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lockfree::channel::spmc::create;
use sling::RingBuffer;
const MAX_SPIN: usize = 128;
const BUF_LEN: usize = 1024;

fn push_pop_lockfree(t: usize) {
    let (mut writer, reader) = create();

    std::thread::scope(|s| {
        let reader = &reader;

        for _ in 0..t {
            s.spawn(move || loop {
                while let Ok(_) = reader.recv() {}

                let mut counter = 0;
                while reader.recv().is_err() && counter < MAX_SPIN {
                    counter += 1;
                    core::hint::spin_loop();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(1000) {
            writer.send(1);
        }
    })
}

fn push_pop_sling<const N: usize>(queue: RingBuffer<u8, N>, t: usize) {
    let mut writer = queue.try_lock().unwrap();
    let reader = queue.reader();

    std::thread::scope(|s| {
        let reader = &reader;
        for _ in 0..t {
            s.spawn(|| loop {
                while let Some(_) = reader.pop_front() {}

                let mut counter = 0;
                while reader.pop_front().is_none() && counter < MAX_SPIN {
                    counter += 1;
                    core::hint::spin_loop();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(1000) {
            writer.push_back(1);
        }
    })
}

fn push_pop_sling_clone<const N: usize>(queue: RingBuffer<u8, N>, t: usize) {
    let mut writer = queue.try_lock().unwrap();
    let reader = queue.reader();

    std::thread::scope(|s| {
        for _ in 0..t {
            s.spawn(|| loop {
                let reader = reader.clone();
                while let Some(_) = reader.pop_front() {}

                let mut counter = 0;
                while reader.pop_front().is_none() && counter < MAX_SPIN {
                    counter += 1;
                    core::hint::spin_loop();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(1000) {
            writer.push_back(1);
        }
    })
}

fn benchmark(c: &mut Criterion) {
    c.bench_function("sling 4", |b| {
        b.iter(|| push_pop_sling(RingBuffer::<u8, BUF_LEN>::new(), 4))
    });

    c.bench_function("sling 8", |b| {
        b.iter(|| push_pop_sling(RingBuffer::<u8, BUF_LEN>::new(), 8))
    });

    c.bench_function("sling 16", |b| {
        b.iter(|| push_pop_sling(RingBuffer::<u8, BUF_LEN>::new(), 16))
    });

    c.bench_function("sling clone 4", |b| {
        b.iter(|| push_pop_sling_clone(RingBuffer::<u8, BUF_LEN>::new(), 4))
    });

    c.bench_function("sling clone 8", |b| {
        b.iter(|| push_pop_sling_clone(RingBuffer::<u8, BUF_LEN>::new(), 8))
    });

    c.bench_function("sling clone 16", |b| {
        b.iter(|| push_pop_sling_clone(RingBuffer::<u8, BUF_LEN>::new(), 16))
    });

    c.bench_function("lockfree 4", |b| b.iter(|| push_pop_lockfree(4)));

    c.bench_function("lockfree 8", |b| b.iter(|| push_pop_lockfree(8)));

    c.bench_function("lockfree 16", |b| b.iter(|| push_pop_lockfree(16)));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
