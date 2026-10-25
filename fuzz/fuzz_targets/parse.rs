// SPDX-License-Identifier: MPL-2.0
#![no_main]

use ijson_jcs::{JsonMode, Value, parse_json_bytes, to_jcs};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for mode in [JsonMode::Strict, JsonMode::Canonical] {
        if let Ok(value) = parse_json_bytes(data, mode) {
            let canonical = to_jcs(&value).expect("accepted value must serialize");
            // JCS must remain JSON even outside the closed strict-token subset.
            serde_json::from_slice::<Value>(&canonical).expect("JCS must be valid JSON");
            if let Ok(closed) = parse_json_bytes(&canonical, JsonMode::Canonical) {
                assert_eq!(canonical, to_jcs(&closed).unwrap());
            }
        }
    }
});
