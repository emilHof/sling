use sling::RingBuffer;

const BUF_LEN: usize = 2_usize.pow(7);
const PAYLOAD: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 0];
const THREADS: [usize; 3] = [1, 2, 4];
const ELEMENTS: usize = 10;

fn push_pop_sling(t: usize) {
    let queue = RingBuffer::<_, BUF_LEN>::new();
    let mut writer = queue.try_lock().unwrap();
    let reader = queue.reader();

    std::thread::scope(|s| {
        let reader = &reader;
        for _ in 0..t {
            s.spawn(move || loop {
                while let Some(_) = reader.pop_front() {}

                std::thread::yield_now();

                if reader.pop_front().is_none() {
                    break;
                }
            });
        }

        for _ in 0..ELEMENTS {
            writer.push_back(PAYLOAD);
            std::thread::yield_now();
        }
    });
}

#[test]
fn loom() {
    loom::model(|| {
        THREADS.iter().for_each(|&t| push_pop_sling(t));
    })
}
