// SPDX-License-Identifier: MPL-2.0
//! Deterministic binary64 corpus for an independent ECMAScript runtime oracle.

use ijson_jcs::{Value, to_jcs_string};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut bits = 0x494a_534f_4e4a_4353u64;
    for _ in 0..20_000 {
        // Deterministic sampling, not a cryptographic generator or proof.
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let f = f64::from_bits(bits);
        if let Some(number) = serde_json::Number::from_f64(f) {
            let canonical = to_jcs_string(&Value::Number(number))?;
            let row = serde_json::json!([format!("{bits:016x}"), canonical]);
            writeln!(out, "{}", to_jcs_string(&row)?)?;
        }
    }
    out.flush()?;
    Ok(())
}
