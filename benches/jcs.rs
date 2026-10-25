// SPDX-License-Identifier: MPL-2.0
//! Frozen workloads emitting the actual proven-bench v1 format.
//! Measurements are empirical; no performance claim exists without a baseline.

use ijson_jcs::{JsonMode, Value, parse_json, to_jcs, validate_i_json};
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u64 = 2000;
const REPS: usize = 9;

fn measure(name: &str, mut workload: impl FnMut() -> bool) -> Value {
    for _ in 0..100 {
        assert!(black_box(workload()), "warmup checksum: {name}");
    }
    let mut samples = Vec::with_capacity(REPS);
    let mut checksum = true;
    for _ in 0..REPS {
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            checksum &= black_box(workload());
        }
        let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap();
        assert!(elapsed > 0, "clock did not measure {name}");
        samples.push(elapsed);
    }
    assert!(checksum, "benchmark payload is broken: {name}");
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    json!({"name": name, "iterations": ITERATIONS, "samples_ns": samples,
           "median_ns": sorted[REPS / 2], "checksum": checksum})
}

fn main() {
    let input = r#"{ "z": [3,2,1], "a": { "x": -0.0, "s": "value" } }"#;
    let expected = br#"{"a":{"s":"value","x":0},"z":[3,2,1]}"#;
    let value = parse_json(input, JsonMode::Canonical).unwrap();
    let number = Value::Number(serde_json::Number::from_f64(f64::from_bits(0x43143ff3c1cb0959)).unwrap());
    let results = vec![
        measure("parse-strict-v1", || parse_json(black_box(input), JsonMode::Strict).unwrap()["z"] == json!([3,2,1])),
        measure("validate-value-v1", || validate_i_json(black_box(&value)).is_ok()),
        measure("canonical-object-v1", || to_jcs(black_box(&value)).unwrap() == expected),
        measure("canonical-number-v1", || to_jcs(black_box(&number)).unwrap() == b"1424953923781206.2"),
    ];
    let report = json!({
        "schema_version": 1, "suite": "proven-bench", "reps": REPS,
        "subject": "ijson-jcs", "workload_revision": 1,
        "commit": std::env::var("GITHUB_SHA").unwrap_or_else(|_| "local-unattested".into()),
        "runner": std::env::var("RUNNER_OS").unwrap_or_else(|_| std::env::consts::OS.into()),
        "results": results,
    });
    let mut bytes = to_jcs(&report).unwrap();
    bytes.push(b'\n');
    if let Ok(path) = std::env::var("IJSON_BENCH_OUTPUT") {
        std::fs::write(path, bytes).unwrap();
    } else {
        use std::io::Write;
        std::io::stdout().lock().write_all(&bytes).unwrap();
    }
}
