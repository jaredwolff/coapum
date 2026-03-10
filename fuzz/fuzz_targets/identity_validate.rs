#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let result = coapum::test_utils::extract_identity(data);
    if let Some(ref id) = result {
        assert!(id.len() <= 256);
        assert!(!id.is_empty());
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()
            || c == '_'
            || c == '-'
            || c == '.'
            || c == ':'));
    }
});
