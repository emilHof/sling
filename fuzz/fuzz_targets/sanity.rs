#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use sling::RingBuffer;

const VALID: [u8; 16] = [
    00, 01, 02, 03, 04, 05, 06, 07, 08, 09, 10, 11, 12, 13, 14, 15,
];

#[derive(Debug, Clone, Copy, Arbitrary)]
enum Message {
    A,
    B,
    C,
    D,
}

fn range(m: Message) -> [u8; 4] {
    let mut empty = [0; 4];
    match m {
        Message::A => empty.copy_from_slice(&VALID[..4]),
        Message::B => empty.copy_from_slice(&VALID[4..8]),
        Message::C => empty.copy_from_slice(&VALID[8..12]),
        Message::D => empty.copy_from_slice(&VALID[12..]),
    }
    empty
}

const ALL: [Message; 4] = [Message::A, Message::B, Message::C, Message::D];

const MAX_SPIN: usize = 64;
const BUFF_SIZE: usize = u8::MAX as usize;

fuzz_target!(|data: Vec<Message>| {
    // fuzzed code goes here
    let buffer = RingBuffer::<[u8; 4], BUFF_SIZE>::new();
    let mut writer = buffer.try_lock().unwrap();
    let mut reader = buffer.reader();
    let mut valid = std::collections::HashSet::new();
    ALL.iter().for_each(|m| {
        valid.insert(range(m.clone()));
    });

    std::thread::scope(|s| {
        let reader = &reader;
        let valid = &valid;

        for _ in 0..8 {
            s.spawn(move || loop {
                while let Some(m) = reader.pop_front() {
                    assert!(valid.contains(&m));
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

        for window in data.windows(8) {
            for &message in window {
                writer.push_back(range(message));
            }
            std::thread::yield_now();
        }
    })
});
