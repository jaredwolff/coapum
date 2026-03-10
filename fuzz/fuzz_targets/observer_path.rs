#![no_main]
use coapum::observer::validate_observer_path;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: String| {
    let result = validate_observer_path(&data);
    if let Ok(ref path) = result {
        assert!(path.starts_with('/'));
        assert!(!path.contains(".."));
        assert!(!path.contains('\\'));
        assert!(path.split('/').count() <= 11);
    }
});
