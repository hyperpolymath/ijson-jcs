// SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
// Code is MPL-2.0; prose is CC-BY-SA-4.0.
//! Shared helpers for the counted-evidence suites (`proven_*`).
#![allow(dead_code)]

pub mod sha256;

use ijson_jcs::Value;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Recorded seed for every counted property run ("IJSONPRV").
pub const SEED: u64 = 0x494a_534f_4e50_5256;

/// Counted property configuration: fixed seed, at least 100 cases.
pub fn config(cases: u32) -> ProptestConfig {
    assert!(cases >= 100, "counted properties need at least 100 cases");
    ProptestConfig {
        cases,
        rng_seed: RngSeed::Fixed(SEED),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// Strings free of Unicode noncharacters (which strict parsing refuses).
pub fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..12).prop_map(|chars| {
        chars
            .into_iter()
            .filter(|c| {
                let n = *c as u32;
                !(0xfdd0..=0xfdef).contains(&n) && n & 0xffff < 0xfffe
            })
            .collect()
    })
}

/// Values closed under the strict integer-token rule (|float| < 1e15).
pub fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-9_007_199_254_740_991i64..=9_007_199_254_740_991).prop_map(|n| Value::Number(n.into())),
        (-1e15f64..1e15).prop_map(|f| Value::Number(serde_json::Number::from_f64(f).unwrap())),
        text().prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 32, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::btree_map(text(), inner, 0..6)
                .prop_map(|entries| Value::Object(entries.into_iter().collect())),
        ]
    })
}

/// A fresh, empty scratch directory under the system temp dir.
pub fn scratch(tag: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ijson-jcs-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the CLI in `dir` with `args` and `stdin`.
pub fn run(dir: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ijson-jcs");
    // The CLI may exit before reading stdin (usage errors): ignore EPIPE.
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}
