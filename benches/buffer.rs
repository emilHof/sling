use std::sync::atomic::{AtomicBool, AtomicUsize};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use crossbeam::channel::{bounded, unbounded};
use lockfree::channel::spmc::create;
use sling::RingBuffer;
const BUF_LEN: usize = 2_usize.pow(8);
const PAYLOAD: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
const THREADS: [usize; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
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

                while reader.try_recv().is_err() && counter < MAX_SPIN {
                    counter += 1;
                    std::thread::yield_now();
                }

                if counter < MAX_SPIN {
                    continue;
                }

                break;
            });
        }

        for _ in 0..black_box(ELEMENTS / 20) {
            for _ in 0..20 {
                writer.try_send(PAYLOAD);
            }
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

        for _ in 0..black_box(ELEMENTS / 20) {
            for _ in 0..20 {
                writer.send(PAYLOAD);
            }
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

        for _ in 0..black_box(ELEMENTS / 20) {
            for _ in 0..20 {
                writer.push_back(PAYLOAD);
            }
            std::thread::yield_now();
        }
    });
}

fn sling_ping(t: usize) {
    let q = RingBuffer::<_, BUF_LEN>::new();

    let mut writer = q.try_lock().unwrap();
    let reader = q.reader();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let reader = &reader;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if reader.pop_front().is_some() {
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        writer.push_back(PAYLOAD);
    });
}

fn crossbeam_ping(t: usize) {
    let (writer, reader) = unbounded();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let reader = &reader;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if reader.try_recv().is_ok() {
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        writer.try_send(PAYLOAD);
    });
}

fn lockfree_ping(t: usize) {
    let (mut writer, reader) = create();

    let pinged = AtomicBool::new(false);

    std::thread::scope(|s| {
        let reader = &reader;
        let pinged = &pinged;

        for _ in 0..t {
            s.spawn(|| {
                while !pinged.load(std::sync::atomic::Ordering::Acquire) {
                    if reader.recv().is_ok() {
                        pinged.store(true, std::sync::atomic::Ordering::Release);
                    }
                    std::thread::yield_now();
                }
            });
        }

        writer.send(PAYLOAD);
    });
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("Bench Throughput With Variable Threads"));
    THREADS.into_iter().for_each(|t| {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("Sling {t} Thread(s)")),
            &t,
            |b, &t| b.iter(|| push_pop_sling(t)),
        );
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("Crossbeam Channel {t} Thread(s)")),
            &t,
            |b, &t| b.iter(|| push_pop_crossbeam(t)),
        );
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("Lockfree Channel {t} Thread(s)")),
            &t,
            |b, &t| b.iter(|| push_pop_lockfree(t)),
        );
    });
    group.finish();
}

fn bench_sling(c: &mut Criterion) {
    let mut group = c.benchmark_group("Bench Sling at Variable Threads".to_string());

    THREADS.into_iter().for_each(|t| {
        group.bench_with_input(BenchmarkId::from_parameter(t), &t, |b, &t| {
            b.iter(|| push_pop_sling(t))
        });
    });
    group.finish();
}

fn bench_ping(c: &mut Criterion) {
    let mut group = c.benchmark_group("Bench Sling Ping Variable Threads".to_string());

    THREADS.into_iter().for_each(|t| {
        group.bench_with_input(format!("Sling {t} Thread(s)"), &t, |b, &t| {
            b.iter(|| sling_ping(t))
        });
        group.bench_with_input(format!("Crossbeam {t} Thread(s)"), &t, |b, &t| {
            b.iter(|| crossbeam_ping(t))
        });
        group.bench_with_input(format!("Lockfree {t} Thread(s)"), &t, |b, &t| {
            b.iter(|| lockfree_ping(t))
        });
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_group!(bench_variable_threads, bench_sling);
criterion_group!(bench_ping_threads, bench_ping);

criterion_main!(bench_variable_threads, benches, bench_ping_threads,);
