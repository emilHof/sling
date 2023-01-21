use std::sync::atomic::{AtomicBool, AtomicUsize};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lockfree::channel::spmc::create;
use sling::RingBuffer;
const BUF_LEN: usize = 2_usize.pow(12);
const PAYLOAD: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
const MAX_SPIN: usize = 128;
const ELEMENTS: usize = 100_000;

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
            writer.send(PAYLOAD);
        }
    });
}

fn push_pop_sling(t: usize) {
    let queue = RingBuffer::<_, BUF_LEN>::new();
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
            writer.push_back(PAYLOAD);
        }
    });
}

fn push_pop_sling_clone(t: usize) {
    let queue = RingBuffer::<_, BUF_LEN>::new();
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
            writer.push_back([PAYLOAD]);
        }
    });
}

fn sling_ping(t: usize) {
    let q1 = RingBuffer::<_, BUF_LEN>::new();
    let q2 = RingBuffer::<_, BUF_LEN>::new();

    let mut w1 = q1.try_lock().unwrap();
    let r1 = q1.reader();
    let r2 = q2.reader();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let r1 = &r1;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some(_) = r1.pop_front() {
                        q2.try_lock().unwrap().push_back(PAYLOAD);
                    }

                    pinged.store(true, std::sync::atomic::Ordering::Release);
                }
            });
        }

        w1.push_back(PAYLOAD);
        while let None = r2.pop_front() {
            core::hint::spin_loop();
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

fn bench_sling(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Bench Sling at Variable Threads"));

    [1, 2, 4, 8, 16].into_iter().for_each(|t| {
        group.bench_function(format!("Sling {} Threads", t), |b| {
            b.iter(|| push_pop_sling(t))
        });
    });
    group.finish();
}

fn bench_ping(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Bench Sling Ping Variable Threads"));

    [1, 2, 4, 8, 16].into_iter().for_each(|t| {
        group.bench_function(format!("Sling {} Threads", t), |b| b.iter(|| sling_ping(t)));
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_group!(bench_variable_threads, bench_sling);
criterion_group!(bench_ping_threads, bench_ping);

criterion_main!(/*benches, bench_variable_threads, */ bench_ping_threads);
