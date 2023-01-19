use std::sync::atomic::AtomicUsize;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lockfree::channel::spmc::create;
use sling::RingBuffer;
const BUF_LEN: usize = 4096 * 4;
const MAX_SPIN: usize = 128;
const ELEMENTS: usize = 10_000;

fn push_pop_lockfree(t: usize) {
    let (mut writer, reader) = create();

    let read = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let read = &read;
        let reader = &reader;

        for _ in 0..t {
            s.spawn(move || loop {
                while let Ok(_) = reader.recv() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

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

        for _ in 0..black_box(ELEMENTS) {
            writer.send(1);
        }
    });
}

fn push_pop_sling(t: usize) {
    let queue = RingBuffer::<u8, BUF_LEN>::new();
    let mut writer = queue.try_lock().unwrap();
    let reader = queue.reader();

    let read = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let reader = &reader;
        let read = &read;
        for _ in 0..t {
            s.spawn(move || loop {
                while let Some(_) = reader.pop_front() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

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

        for _ in 0..black_box(ELEMENTS) {
            writer.push_back(1);
        }
    });
}

fn push_pop_sling_clone(t: usize) {
    let queue = RingBuffer::<u8, BUF_LEN>::new();
    let mut writer = queue.try_lock().unwrap();
    let reader = queue.reader();

    let read = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let read = &read;
        for _ in 0..t {
            s.spawn(|| loop {
                let reader = reader.clone();
                while let Some(_) = reader.pop_front() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

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

        for _ in 0..black_box(ELEMENTS) {
            writer.push_back(1);
        }
    });
}

fn bench(c: &mut Criterion) {
    [1, 2, 4, 8, 16].into_iter().for_each(|t| {
        let mut group = c.benchmark_group(format!("Bench {} Thread(s)", t));

        group.bench_function("Sling", |b| b.iter(|| push_pop_sling(t)));
        group.bench_function("Sling Cloned", |b| b.iter(|| push_pop_sling_clone(t)));
        group.bench_function("Lockfree Channel", |b| b.iter(|| push_pop_lockfree(t)));

        group.finish();
    })
}

criterion_group!(benches, bench);
criterion_main!(benches);
