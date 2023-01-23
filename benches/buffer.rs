use std::sync::atomic::{AtomicBool, AtomicUsize};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use crossbeam::channel::{bounded, unbounded};
use lockfree::channel::spmc::create;
use sling::RingBuffer;
const BUF_LEN: usize = 2_usize.pow(8);
const PAYLOAD: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
const THREADS: [usize; 5] = [1, 2, 4, 8, 16];
const MAX_SPIN: usize = 128;
const ELEMENTS: usize = 100_000;

fn push_pop_crossbeam(t: usize) {
    let (writer, reader) = bounded(BUF_LEN);

    let read = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let read = &read;
        let reader = &reader;

        for _ in 0..t {
            s.spawn(move || loop {
                let reader = reader.clone();

                while reader.try_recv().is_ok() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

                let mut counter = 0;

                while reader.recv().is_err() && counter < MAX_SPIN {
                    counter += 1;
                    std::thread::yield_now();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(ELEMENTS) {
            writer.try_send(PAYLOAD);
            std::thread::yield_now();
        }
    });
}

fn push_pop_lockfree(t: usize) {
    let (mut writer, reader) = create();

    let read = AtomicUsize::new(0);
    std::thread::scope(|s| {
        let read = &read;
        let reader = &reader;

        for _ in 0..t {
            s.spawn(move || loop {
                while reader.recv().is_ok() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

                let mut counter = 0;

                while reader.recv().is_err() && counter < MAX_SPIN {
                    counter += 1;
                    std::thread::yield_now();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(ELEMENTS) {
            writer.send(PAYLOAD);
            std::thread::yield_now();
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
                while reader.pop_front().is_some() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

                let mut counter = 0;

                while reader.pop_front().is_none() && counter < MAX_SPIN {
                    counter += 1;
                    std::thread::yield_now();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(ELEMENTS) {
            writer.push_back(PAYLOAD);
            std::thread::yield_now();
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
                while reader.pop_front().is_some() {
                    read.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }

                let mut counter = 0;

                while reader.pop_front().is_none() && counter < MAX_SPIN {
                    counter += 1;
                    std::thread::yield_now();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(ELEMENTS) {
            writer.push_back([PAYLOAD]);
            std::thread::yield_now();
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
                    if r1.pop_front().is_some() {
                        q2.try_lock().unwrap().push_back(PAYLOAD);
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        w1.push_back(PAYLOAD);
        while r2.pop_front().is_none() {
            std::thread::yield_now();
        }
    });
}

fn crossbeam_ping(t: usize) {
    let (w1, r1) = unbounded();
    let (w2, r2) = unbounded();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let r1 = &r1;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if r1.try_recv().is_ok() {
                        w2.try_send(PAYLOAD);
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        w1.try_send(PAYLOAD);
        while r2.try_recv().is_err() {
            std::thread::yield_now();
        }
    });
}

fn lockfree_ping(t: usize) {
    let (mut w1, r1) = create();
    let (w2, r2) = create();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let r1 = &r1;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if r1.recv().is_ok() {
                        unsafe {
                            (*(&w2 as *const lockfree::channel::spmc::Sender<[u8; 8]>
                                as *mut lockfree::channel::spmc::Sender<[u8; 8]>))
                                .send(PAYLOAD)
                        };
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        w1.send(PAYLOAD);
        while r2.recv().is_err() {
            std::thread::yield_now();
        }
    });
}

fn bench(c: &mut Criterion) {
    THREADS.into_iter().for_each(|t| {
        let mut group = c.benchmark_group(format!("Bench {t} Thread(s)"));

        group.bench_function("Sling", |b| b.iter(|| push_pop_sling(t)));
        group.bench_function("Sling Cloned", |b| b.iter(|| push_pop_sling_clone(t)));
        group.bench_function("Lockfree Channel", |b| b.iter(|| push_pop_lockfree(t)));
        // group.bench_function("Crossbeam Channel", |b| b.iter(|| push_pop_crossbeam(t)));

        group.finish();
    })
}

fn bench_sling(c: &mut Criterion) {
    let mut group = c.benchmark_group("Bench Sling at Variable Threads".to_string());

    THREADS.into_iter().for_each(|t| {
        group.bench_function(format!("Sling {t} Threads"), |b| {
            b.iter(|| push_pop_sling(t))
        });
    });
    group.finish();
}

fn bench_ping(c: &mut Criterion) {
    let mut group = c.benchmark_group("Bench Sling Ping Variable Threads".to_string());

    THREADS.into_iter().for_each(|t| {
        group.bench_function(format!("Sling {t} Threads"), |b| b.iter(|| sling_ping(t)));
        group.bench_function(format!("Crossbeam {t} Threads"), |b| {
            b.iter(|| crossbeam_ping(t))
        });
        group.bench_function(format!("Lockfree {t} Threads"), |b| {
            b.iter(|| lockfree_ping(t))
        });
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_group!(bench_variable_threads, bench_sling);
criterion_group!(bench_ping_threads, bench_ping);

criterion_main!(/*benches, bench_variable_threads, */ bench_ping_threads);
