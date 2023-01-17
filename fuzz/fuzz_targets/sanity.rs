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
    E,
}

fn range(m: Message) -> [u8; 5] {
    let mut empty = [0; 5];
    match m {
        Message::A => empty.copy_from_slice(&VALID[0..5]),
        Message::B => empty.copy_from_slice(&VALID[2..7]),
        Message::C => empty.copy_from_slice(&VALID[4..9]),
        Message::D => empty.copy_from_slice(&VALID[6..11]),
        Message::E => empty.copy_from_slice(&VALID[8..13]),
    }
    empty
}

const ALL: [Message; 5] = [Message::A, Message::B, Message::C, Message::D, Message::E];

const MAX_SPIN: usize = 64;
const BUFF_SIZE: usize = u8::MAX as usize;

fuzz_target!(|data: Vec<Message>| {
    // fuzzed code goes here
    let buffer = RingBuffer::<[u8; 5], BUFF_SIZE>::new();
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
                    println!("{:?}", m);
                    assert!(valid.contains(&m));
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

        for message in data {
            writer.push_back(range(message));
        }
    })
});
