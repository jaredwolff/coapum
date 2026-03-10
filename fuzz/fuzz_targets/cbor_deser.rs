#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }
    let _ = ciborium::de::from_reader_with_recursion_limit::<ciborium::Value, _>(data, 32);
});
