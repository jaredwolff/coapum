#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 1_048_576 {
        return;
    }
    let _ = serde_json::from_slice::<serde_json::Value>(data);
});
