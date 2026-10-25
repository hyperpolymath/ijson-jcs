// SPDX-License-Identifier: MPL-2.0
#![no_main]

use ijson_jcs::{Value, to_jcs};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bits: u64| {
    let number = f64::from_bits(bits);
    if !number.is_finite() {
        return;
    }
    let canonical = to_jcs(&Value::from(number)).unwrap();
    let parsed: serde_json::Number = serde_json::from_slice(&canonical).unwrap();
    let restored = parsed.as_f64().unwrap();
    if number == 0.0 {
        assert_eq!(canonical, b"0");
        assert_eq!(restored, 0.0);
    } else {
        assert_eq!(number.to_bits(), restored.to_bits());
    }
});
