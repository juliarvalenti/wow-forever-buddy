//! SavedVariables parser against fixtures: a differential test with
//! `full_moon` as the oracle, round-trips, and a proptest round-trip.

use full_moon::ast::{BinOp, Expression, Field, Stmt, UnOp, Var};
use full_moon::tokenizer::{StringLiteralQuoteType, Symbol, TokenType};
use proptest::prelude::*;
use std::path::PathBuf;
use wow_forever_buddy_lib::sv::{self, Globals, LuaTable, LuaValue};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sv")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Valid fixtures that are also plain Lua 5.1, so full_moon can parse them.
/// crlf_bom.lua is valid Lua, but full_moon's tokenizer rejects a backslash
/// followed by CRLF inside a string, so it's covered by `crlf_and_bom` only.
const LUA_FIXTURES: &[&str] = &[
    "details.lua",
    "weakauras.lua",
    "auctionator.lua",
    "strings.lua",
];

/// Every fixture that should parse.
const VALID_FIXTURES: &[&str] = &[
    "details.lua",
    "weakauras.lua",
    "auctionator.lua",
    "strings.lua",
    "crlf_bom.lua",
    "non_utf8.lua",
    "special_floats.lua",
];

// --- full_moon oracle ------------------------------------------------------
//
// Converts full_moon's AST to our model with its own number and escape
// handling, so the two sides share no parsing code.

fn oracle(src: &str) -> Globals {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let ast = full_moon::parse(src).unwrap_or_else(|e| panic!("full_moon rejected fixture: {e:?}"));
    ast.nodes()
        .stmts()
        .map(|stmt| match stmt {
            Stmt::Assignment(a) => {
                let vars: Vec<_> = a.variables().iter().collect();
                let exprs: Vec<_> = a.expressions().iter().collect();
                assert_eq!(
                    (vars.len(), exprs.len()),
                    (1, 1),
                    "multiple assignment in fixture"
                );
                let Var::Name(name) = vars[0] else {
                    panic!("non-name assignment target")
                };
                (name.token().to_string(), oracle_expr(exprs[0]))
            }
            other => panic!("unexpected statement: {other}"),
        })
        .collect()
}

fn oracle_expr(e: &Expression) -> LuaValue {
    match e {
        Expression::Number(t) => oracle_number(&t.token().to_string()),
        Expression::String(t) => match t.token_type() {
            TokenType::StringLiteral {
                literal,
                quote_type,
                ..
            } => {
                if *quote_type == StringLiteralQuoteType::Brackets {
                    LuaValue::str(oracle_long(literal.as_str()))
                } else {
                    LuaValue::str(oracle_unescape(literal.as_str()))
                }
            }
            other => panic!("unexpected string token {other:?}"),
        },
        Expression::Symbol(t) => match t.token_type() {
            TokenType::Symbol {
                symbol: Symbol::True,
            } => LuaValue::Bool(true),
            TokenType::Symbol {
                symbol: Symbol::False,
            } => LuaValue::Bool(false),
            TokenType::Symbol {
                symbol: Symbol::Nil,
            } => LuaValue::Nil,
            other => panic!("unexpected symbol {other:?}"),
        },
        Expression::UnaryOperator {
            unop: UnOp::Minus(_),
            expression,
        } => match &**expression {
            // Negate the literal text so -9223372036854775808 stays an integer.
            Expression::Number(t) => oracle_number(&format!("-{}", t.token())),
            other => match oracle_expr(other) {
                LuaValue::Num(n) => LuaValue::Num(-n),
                v => panic!("negated non-number {v:?}"),
            },
        },
        Expression::BinaryOperator {
            lhs,
            binop: BinOp::Slash(_),
            rhs,
        } => {
            let (a, b) = (
                oracle_expr(lhs).as_f64().unwrap(),
                oracle_expr(rhs).as_f64().unwrap(),
            );
            LuaValue::Num(a / b)
        }
        Expression::TableConstructor(t) => {
            let mut table = LuaTable::default();
            for field in t.fields().iter() {
                match field {
                    Field::ExpressionKey { key, value, .. } => {
                        table.hash.push((oracle_expr(key), oracle_expr(value)))
                    }
                    Field::NameKey { key, value, .. } => table
                        .hash
                        .push((LuaValue::str(key.token().to_string()), oracle_expr(value))),
                    Field::NoKey(value) => table.array.push(oracle_expr(value)),
                    other => panic!("unexpected field {other}"),
                }
            }
            LuaValue::Table(Box::new(table))
        }
        other => panic!("unexpected expression: {other}"),
    }
}

fn oracle_number(text: &str) -> LuaValue {
    let (neg, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        let v = i64::from_str_radix(hex, 16).expect("hex fixture fits i64");
        return LuaValue::Int(if neg { -v } else { v });
    }
    if !body.contains(['.', 'e', 'E']) {
        if let Ok(i) = text.parse::<i64>() {
            return LuaValue::Int(i);
        }
    }
    LuaValue::Num(text.parse().expect("number"))
}

fn oracle_unescape(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'\r' | b'\n' => {
                out.push(b'\n');
                if matches!(b.get(i + 1), Some(b'\r' | b'\n')) && b[i + 1] != b[i] {
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                let n = b[i..]
                    .iter()
                    .take(3)
                    .take_while(|d| d.is_ascii_digit())
                    .count();
                out.push(s[i..i + n].parse::<u8>().unwrap());
                i += n;
                continue;
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

fn oracle_long(s: &str) -> String {
    let s = s
        .strip_prefix("\r\n")
        .or_else(|| s.strip_prefix('\n'))
        .unwrap_or(s);
    s.replace("\r\n", "\n")
}

// --- tests -------------------------------------------------------------------

#[test]
fn differential_vs_full_moon() {
    for name in LUA_FIXTURES {
        let bytes = fixture(name);
        let ours = sv::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let theirs = oracle(std::str::from_utf8(&bytes).unwrap());
        assert!(!ours.is_empty(), "{name}: no globals");
        assert_eq!(ours, theirs, "{name}");
    }
}

#[test]
fn fixtures_round_trip() {
    for name in VALID_FIXTURES {
        let first = sv::parse(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let written = sv::write_globals(&first);
        let second = sv::parse(&written).unwrap_or_else(|e| panic!("{name} reparse: {e}"));
        assert_eq!(first, second, "{name}");
    }
}

#[test]
fn truncated_fixture_is_a_parse_error() {
    let bytes = fixture("truncated.lua");
    let err = sv::parse(&bytes).unwrap_err();
    let last_line = bytes.split(|&b| b == b'\n').count() as u32;
    assert_eq!(err.line, last_line, "{err}");
    assert!(err.msg.contains("unexpected identifier `tr`"), "{err}");
}

#[test]
fn every_truncation_is_an_error_or_a_prefix() {
    // A torn read can stop anywhere. Parsing must never panic, and any cut
    // inside the table must be an error, never a silently shorter table.
    let bytes = fixture("weakauras.lua");
    let full = sv::parse(&bytes).unwrap();
    let body_end = bytes.iter().rposition(|&b| b == b'}').unwrap();
    for cut in 0..bytes.len() {
        let result = sv::parse(&bytes[..cut]);
        if cut > body_end {
            assert_eq!(result.unwrap(), full);
        } else if let Ok(globals) = result {
            // Only a cut before the value even starts can succeed.
            assert!(globals.is_empty(), "cut at {cut} parsed: {globals:?}");
        }
    }
}

#[test]
fn non_utf8_bytes_survive() {
    let g = sv::parse(&fixture("non_utf8.lua")).unwrap();
    let db = g[0].1.as_table().unwrap();
    assert_eq!(
        db.get("name").unwrap().as_bytes().unwrap(),
        b"Th\xe9r\xe8se"
    );
    assert_eq!(
        db.get("raw").unwrap().as_bytes().unwrap(),
        b"\xff\xfe\x80\xc3"
    );
    assert_eq!(
        db.get("long").unwrap().as_bytes().unwrap(),
        b"caf\xe9\nau lait"
    );
    assert_eq!(
        db.get("name").unwrap().to_string_lossy().unwrap(),
        "Th\u{fffd}r\u{fffd}se"
    );
}

#[test]
fn crlf_and_bom() {
    let g = sv::parse(&fixture("crlf_bom.lua")).unwrap();
    assert_eq!(g[0].0, "CrlfDB");
    let db = g[0].1.as_table().unwrap();
    assert_eq!(db.get("a"), Some(&LuaValue::str("x\ny")));
    assert_eq!(db.get("long"), Some(&LuaValue::str("one\ntwo")));
}

#[test]
fn special_floats() {
    let g = sv::parse(&fixture("special_floats.lua")).unwrap();
    let db = g[0].1.as_table().unwrap();
    let f = |k: &str| db.get(k).unwrap().as_f64().unwrap();
    for k in ["inf", "div", "msvc_inf"] {
        assert_eq!(f(k), f64::INFINITY, "{k}");
    }
    for k in ["neg_inf", "neg_div"] {
        assert_eq!(f(k), f64::NEG_INFINITY, "{k}");
    }
    for k in ["nan_div", "msvc_ind", "msvc_qnan", "ucrt_nan", "glibc_nan"] {
        assert!(f(k).is_nan(), "{k}");
    }
}

#[test]
fn spot_checks() {
    let g = sv::parse(&fixture("strings.lua")).unwrap();
    let db = g[0].1.as_table().unwrap();
    let s = |k: &str| db.get(k).unwrap().to_string_lossy().unwrap();
    assert_eq!(s("percent_q_newline"), "line one\nline two");
    // Lua's \ddd is decimal, not octal: \033 is '!'.
    assert_eq!(s("decimal"), "Hi!1\0");
    assert_eq!(s("utf8"), "Thrandor — Ëlune's Grace — 龍");
    assert_eq!(s("long_level"), "contains ]] and ]=] before the end");
    let numbers = &db.get("numbers").unwrap().as_table().unwrap().array;
    assert_eq!(numbers[6], LuaValue::Int(9007199254740993));
    assert_eq!(numbers[7], LuaValue::Int(i64::MIN));
    assert_eq!(g[1], ("StringsVersion".to_string(), LuaValue::Int(2)));

    let g = sv::parse(&fixture("details.lua")).unwrap();
    let actor = &g[1]
        .1
        .as_table()
        .unwrap()
        .get("tabela_historico")
        .unwrap()
        .as_table()
        .unwrap()
        .get("tabelas")
        .unwrap()
        .as_table()
        .unwrap()
        .array[0]
        .as_table()
        .unwrap()
        .array[0]
        .as_table()
        .unwrap()
        .get("_ActorTable")
        .unwrap()
        .as_table()
        .unwrap()
        .array[0];
    assert_eq!(
        actor.as_table().unwrap().get("nome"),
        Some(&LuaValue::str("Thrandor"))
    );
}

// --- proptest round-trip -----------------------------------------------------

fn scalar() -> impl Strategy<Value = LuaValue> {
    prop_oneof![
        Just(LuaValue::Nil),
        any::<bool>().prop_map(LuaValue::Bool),
        any::<i64>().prop_map(LuaValue::Int),
        any::<f64>().prop_map(LuaValue::Num),
        prop_oneof![
            Just(f64::NAN),
            Just(f64::INFINITY),
            Just(f64::NEG_INFINITY),
            Just(-0.0)
        ]
        .prop_map(LuaValue::Num),
        proptest::collection::vec(any::<u8>(), 0..32).prop_map(LuaValue::str),
        "\\PC{0,16}".prop_map(LuaValue::str),
    ]
}

fn value() -> impl Strategy<Value = LuaValue> {
    scalar().prop_recursive(5, 128, 8, |inner| {
        (
            proptest::collection::vec(inner.clone(), 0..8),
            proptest::collection::vec((scalar(), inner), 0..8),
        )
            .prop_map(|(array, hash)| LuaValue::Table(Box::new(LuaTable { array, hash })))
    })
}

proptest! {
    #[test]
    fn generated_values_round_trip(globals in proptest::collection::vec(("[A-Za-z_][A-Za-z0-9_]{0,12}", value()), 0..4)) {
        let written = sv::write_globals(&globals);
        let parsed = sv::parse(&written).map_err(|e| TestCaseError::fail(format!("{e}\n{}", String::from_utf8_lossy(&written))))?;
        prop_assert_eq!(parsed, globals);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = sv::parse(&bytes);
    }
}
