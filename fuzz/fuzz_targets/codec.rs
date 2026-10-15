// Codec round-trip + robustness fuzz target: every event stream reachable
// from arbitrary bytes must survive encode → decode losslessly, and arbitrary
// bytes must never panic (or over-allocate) any decoder.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vtty::{Event, Parser, State};
use vtty_codec::{Encode, decode_from_slice, encode_to_vec};

fuzz_target!(|data: &[u8]| {
    // 1. Event stream round-trip.
    let mut parser = Parser::new();
    let events = parser.parse_events(data);
    let bytes = encode_to_vec(&events).expect("event encode");
    assert_eq!(bytes.len(), events.encoded_len(), "event encoded_len");
    assert_eq!(
        decode_from_slice::<Vec<Event>>(&bytes).expect("event decode"),
        events,
    );

    // 2. Byte-cursor state round-trip.
    let mut state = State::new();
    let mut p2 = Parser::new();
    p2.advance(&mut state, data);
    let bytes = encode_to_vec(&state).expect("state encode");
    assert_eq!(bytes.len(), state.encoded_len(), "state encoded_len");
    assert_eq!(
        decode_from_slice::<State>(&bytes).expect("state decode"),
        state,
    );

    // 3. Robustness: arbitrary bytes never panic or over-allocate a decoder.
    let _ = decode_from_slice::<Event>(data);
    let _ = decode_from_slice::<Vec<Event>>(data);
    let _ = decode_from_slice::<State>(data);
});
