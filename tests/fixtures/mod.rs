// SPDX-License-Identifier: MPL-2.0
//! Paired clean/fault specimens: harness silence AND payload firing.

pub struct Pair {
    pub class: &'static str,
    pub clean: &'static [u8],
    pub fault: &'static [u8],
    pub diagnostic: &'static str,
}

pub const PAIRS: &[Pair] = &[
    Pair {
        class: "duplicate decoded keys",
        clean: br#"{"a":1,"b":2}"#,
        fault: br#"{"a":1,"\u0061":2}"#,
        diagnostic: "duplicate object key",
    },
    Pair {
        class: "duplicate nested keys",
        clean: br#"[{"k":1},{"k":2}]"#,
        fault: br#"[{"k":1,"k":2}]"#,
        diagnostic: "duplicate object key",
    },
    Pair {
        class: "surrogates",
        clean: br#""\ud800\udc00""#,
        fault: br#""\ud800""#,
        diagnostic: "hex escape",
    },
    Pair {
        class: "noncharacters",
        clean: br#""\ufffd""#,
        fault: br#""\uffff""#,
        diagnostic: "noncharacter",
    },
    Pair {
        class: "safe integer boundary",
        clean: b"9007199254740991",
        fault: b"9007199254740992",
        diagnostic: "outside safe integer range",
    },
    Pair {
        class: "integer token beyond u64",
        clean: br#""18446744073709551616""#,
        fault: b"18446744073709551616",
        diagnostic: "outside safe integer range",
    },
    Pair {
        class: "float overflow",
        clean: b"1e308",
        fault: b"1e400",
        diagnostic: "number out of range",
    },
    Pair {
        class: "UTF-8",
        clean: b"\"hello\"",
        fault: b"\"\xff\"",
        diagnostic: "encoding error",
    },
    Pair {
        class: "trailing data",
        clean: b"{} \n",
        fault: b"{} []",
        diagnostic: "trailing characters",
    },
    Pair {
        class: "truncation",
        clean: b"[1]",
        fault: b"[1",
        diagnostic: "EOF",
    },
];
