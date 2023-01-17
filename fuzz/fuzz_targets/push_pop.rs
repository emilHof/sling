#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sling::RingBuffer;

#[derive(Debug, Clone, Copy, Arbitrary)]
struct Message {
    time: [u8; 16],
    price: u32,
    action: Action,
}

#[derive(Debug, Clone, Copy, Arbitrary)]
enum Action {
    Buy,
    Sell,
}

const MAX_SPIN: usize = 64;
const BUFF_SIZE: usize = u8::MAX as usize;

fuzz_target!(|data: Vec<Message>| {
    // fuzzed code goes here
    let buffer = RingBuffer::<_, BUFF_SIZE>::new();
    let mut writer = buffer.try_lock().unwrap();
    let mut reader = buffer.reader();

    std::thread::scope(|s| {
        for _ in 0..16 {
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

        for message in data {
            writer.push_back(message);
        }
    })
});
