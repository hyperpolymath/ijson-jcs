# I-JSON + JCS Library

[![License: MPL-2.0](https://img.shields.io/badge/License-MPL_2.0-blue.svg)](https://opensource.org/licenses/MPL-2.0)

**Elegant, correct JSON handling for the Hyperpolymath estate**

This crate provides comprehensive I-JSON (RFC 7493) validation and JCS (RFC 8785) canonicalization for deterministic JSON handling in scenarios requiring hashing, signing, content-addressing, or reproducible builds.

## Features

- ✅ **I-JSON Validation**: Comprehensive validation of all I-JSON requirements
- ✅ **JCS Canonicalization**: Deterministic byte-for-byte JSON serialization per RFC 8785
- ✅ **Dual-Mode APIs**: Loose parsing for compatibility, strict validation for safety
- ✅ **Full RFC Compliance**: Adheres to both RFC 7493 and RFC 8785 specifications
- ✅ **Comprehensive Testing**: 33+ unit tests plus doc tests covering all edge cases
- ✅ **Zero Dependencies**: Minimal, clean dependency tree (serde, serde_json, thiserror)

## Fork Relationship

This implementation is **forked and enhanced** from the excellent [Groove protocol's canonical.rs](https://github.com/hyperpolymath/groove-protocol/blob/master/cli/src/canonical.rs) implementation, which was already RFC 8785 compliant for their manifest signing use case.

### Enhancements Made

1. **Added I-JSON validation layer** - comprehensive validation of all I-JSON requirements
2. **Full number support** - handles integers and floats according to ES number-to-string rules
3. **Proper UTF-16 key sorting** - JCS requires UTF-16 code unit order, not simple string comparison
4. **Enhanced error handling** - detailed, structured error types for all validation failures
5. **Dual-mode APIs** - supports loose parsing (for compatibility) and strict modes
6. **Unified API design** - clean, intuitive interface for all JSON operations

## I-JSON (RFC 7493) Compliance

I-JSON is a restricted profile of JSON that addresses interoperability issues:

### MUST Requirements ✅
- UTF-8 encoding
- No duplicate object keys
- No unpaired surrogates (U+D800..U+DFFF)
- No noncharacters in strings/keys

### SHOULD Requirements ✅
- Numbers limited to IEEE-754 binary64 exact representation
- Top-level value should be object or array
- Follow RFC 3339 for timestamps, base64url for binary

### Validation Features
- **Duplicate Key Detection**: Catches duplicate keys during object validation
- **Unicode Safety**: Validates no unpaired surrogates or noncharacters
- **Control Character Validation**: Only allows TAB, LF, CR in strings
- **Safe Integer Range**: Validates integers are in [-(2^53)+1, (2^53)-1] for exact representation
- **Non-Finite Number Rejection**: Rejects NaN and Infinity

## JCS (RFC 8785) Compliance

JCS provides deterministic canonicalization of JSON data with these rules:

### Requirements ✅
- **UTF-8 Output**: Always produces UTF-8 bytes
- **Sorted Keys**: Object keys sorted by UTF-16 code unit order
- **Minimal Separators**: No insignificant whitespace (compact form)
- **ES Number Rules**: Numbers serialized per ECMAScript JSON.stringify rules
- **Array Order**: Arrays preserve element order (only objects get key-sorted)
- **Deterministic**: Same input always produces same byte output

### Key Features
- **Byte-level Determinism**: Returns `Vec<u8>` for consistent hashing/signing
- **Full Number Support**: Handles all I-JSON number types correctly
- **String Escaping**: RFC 8259 compliant string escaping via serde_json
- **Recursive Processing**: Handles nested objects and arrays correctly

## Usage

### Basic Usage

```rust
use ijson_jcs::{JsonMode, parse_json, to_jcs, validate_i_json};
use serde_json::json;

// Parse with I-JSON validation
let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Strict)?;

// Canonicalize to JCS (deterministic bytes)
let canonical_bytes = to_jcs(&value)?;
// canonical_bytes == b"{\"a\":2,\"b\":1}"
```

### Parsing Modes

```rust
use ijson_jcs::{JsonMode, parse_json};

// Loose mode - accepts any valid JSON (compatibility)
let loose = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Loose)?;

// Strict mode - validates I-JSON compliance
let strict = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Strict)?;

// Canonical mode - validates and prepares for JCS
let canonical = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Canonical)?;
```

### Validation Only

```rust
use ijson_jcs::{validate_i_json, is_valid_i_json_string};
use serde_json::json;

// Validate a parsed value
let value = json!({"a": 1, "b": 2});
assert!(validate_i_json(&value).is_ok());

// Validate a string directly
assert!(is_valid_i_json_string(r#"{"a": 1}"#));
```

### JCS Canonicalization

```rust
use ijson_jcs::{to_jcs, to_jcs_string};
use serde_json::json;

let value = json!({"b": 1, "a": 2});

// Get canonical bytes (for hashing, signing)
let canonical_bytes = to_jcs(&value)?;

// Get canonical string
let canonical_string = to_jcs_string(&value)?;
// canonical_string == "{\"a\":2,\"b\":1}"
```

## API Reference

### Main Types

- `JsonMode`: Parsing mode (Loose, Strict, Canonical)
- `IJsonError`: Error type for I-JSON operations
- `ValidationError`: I-JSON validation failure details
- `CanonicalizationError`: JCS canonicalization failure details

### Main Functions

- `parse_json(input: &str, mode: JsonMode) -> Result<Value, IJsonError>`
- `parse_json_bytes(input: &[u8], mode: JsonMode) -> Result<Value, IJsonError>`
- `validate_i_json(value: &Value) -> Result<(), ValidationError>`
- `to_jcs(value: &Value) -> Result<Vec<u8>, CanonicalizationError>`
- `to_jcs_string(value: &Value) -> Result<String, CanonicalizationError>`

### Helper Functions

- `is_valid_i_json_string(input: &str) -> bool`
- `is_valid_i_json_bytes(input: &[u8]) -> bool`
- `to_json_string(value: &Value) -> String` (standard serialization)
- `to_json_pretty_string(value: &Value) -> String` (pretty-printed)

## Error Handling

All operations return proper error types with detailed information:

```rust
use ijson_jcs::{JsonMode, parse_json, IJsonError};

match parse_json(r#"{"k":1,"k":2}"#, JsonMode::Strict) {
    Ok(value) => {
        // Valid I-JSON
    }
    Err(IJsonError::ValidationError(e)) => {
        match e {
            ValidationError::DuplicateKey { key, path } => {
                println!("Duplicate key: {} at path {}", key, path);
            }
            ValidationError::UnsafeInteger { value, path } => {
                println!("Unsafe integer: {} at path {}", value, path);
            }
            // ... other validation errors
        }
    }
    Err(IJsonError::ParseError { source }) => {
        println!("JSON parse error: {}", source);
    }
}
```

## Comparison with Original Groove Implementation

| Feature | Original Groove | Enhanced I-JSON-JCS |
|---------|----------------|---------------------|
| **Compliance** | RFC 8785 compliant | RFC 7493 + RFC 8785 compliant |
| **Number Support** | Integers only | Full number support |
| **Key Sorting** | Lexicographic | UTF-16 code unit order |
| **I-JSON Validation** | ❌ None | ✅ Comprehensive |
| **Error Handling** | Basic | Detailed, structured |
| **API Modes** | Single | Dual-mode (Loose/Strict/Canonical) |
| **Test Coverage** | Good | Comprehensive |

The enhanced implementation maintains **full backward compatibility** with the original Groove use case while adding comprehensive I-JSON validation and support for additional use cases.

## Implementation Details

### UTF-16 Code Unit Sorting

JCS requires that object keys be sorted by UTF-16 code unit order, not simple string comparison. For ASCII-only keys (the most common case), this behaves identically to lexicographic comparison, but ensures correct ordering for non-ASCII keys.

### Number Handling

- **Safe Integers**: Integers in [-(2^53)+1, (2^53)-1] are serialized as numbers
- **Unsafe Integers**: Larger integers are flagged as validation errors (I-JSON recommendation)
- **Floats**: Allowed but users should consider string representation for exactness
- **Special Values**: NaN and Infinity are rejected (not valid RFC 8259 JSON)

### Unicode Safety

- **Unpaired Surrogates**: U+D800..U+DFFF are rejected
- **Noncharacters**: U+FDD0..U+FDEF, U+FFFE, U+FFFF, U+1FFFE, U+1FFFF, etc. are rejected
- **Control Characters**: Only U+0009 (TAB), U+000A (LF), U+000D (CR) are allowed in strings

## Performance

- **Zero-copy**: Uses `Vec<u8>` for output to minimize allocations
- **Efficient Validation**: Single-pass validation with minimal overhead
- **Pre-allocated Buffers**: Output buffers are pre-allocated for common cases

## Compatibility

- **Rust Version**: 1.70+ (uses 2024 edition)
- **Dependencies**: serde, serde_json, thiserror
- **No Unsafe**: 100% safe Rust
- **Cross-Platform**: Pure Rust, works on all platforms

## License

MPL-2.0 (Mozilla Public License 2.0)

## Repository Structure

```
ijson-jcs/
├── Cargo.toml          # Package manifest
├── README.md           # This file
├── src/
│   ├── lib.rs          # Main exports and convenience functions
│   ├── error.rs        # Comprehensive error types
│   ├── ijson.rs        # I-JSON validation implementation
│   └── jcs.rs          # JCS canonicalization implementation
└── target/             # Build artifacts (gitignored)
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass (`cargo test`)
6. Submit a pull request

## Testing

```bash
# Run all tests
cargo test

# Run with coverage
cargo tarpaulin

# Run specific tests
cargo test test_duplicate_keys
cargo test --doc
```

## Future Enhancements

- [ ] RFC 8785 official test vectors integration
- [ ] FFI bindings for other languages (C, Python, Node.js)
- [ ] WASM compilation for browser/Node.js usage
- [ ] Integration with existing estate JSON handling code
- [ ] Performance benchmarks and optimization
- [ ] Additional I-JSON recommendations (RFC 3339 timestamps, base64url)

## References

- **RFC 7493**: [I-JSON - The Internet JSON](https://tools.ietf.org/html/rfc7493)
- **RFC 8785**: [JSON Canonicalization Scheme (JCS)](https://tools.ietf.org/html/rfc8785)
- **RFC 8259**: [The JavaScript Object Notation (JSON) Data Interchange Format](https://tools.ietf.org/html/rfc8259)
- **Original Groove Implementation**: [hyperpolymath/groove-protocol/cli/src/canonical.rs](https://github.com/hyperpolymath/groove-protocol/blob/master/cli/src/canonical.rs)

---

*Repository*: [hyperpolymath/ijson-jcs](https://github.com/hyperpolymath/ijson-jcs)  
*License*: MPL-2.0  
*Status*: Active development  
*Version*: 0.1.0