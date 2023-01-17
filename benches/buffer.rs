use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sling::RingBuffer;

fn push_pop_sling<T: Copy, const N: usize>(queue: RingBuffer<T, N>) {
    let writer = queue.try_lock().unwrap();
    let reader = queue.reader();
}

fn benchmark(c: &mut Criterion) {
    c.bench_function("push pop sling", |b| {
        b.iter(|| push_pop_sling(RingBuffer::<u8, 1024>::new()))
    });
}
