#![allow(clippy::expect_used, clippy::panic)]

use bhippi_engine::runtime_protocol::{RuntimeProtocolError, RuntimeProtocolGuard};
use serde::Deserialize;

#[derive(Deserialize)]
struct HostileCase {
    name: String,
    maximum_bytes: usize,
    expected: String,
    message: serde_json::Value,
}

#[test]
fn hostile_protocol_corpus_fails_closed_with_stable_reasons() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/engine/runtime_protocol_hostile/cases.json"
    ));
    let cases: Vec<HostileCase> = serde_json::from_str(fixture).expect("valid hostile corpus");
    assert!(!cases.is_empty());

    for case in cases {
        let encoded = serde_json::to_vec(&case.message).expect("case encodes");
        let mut guard =
            RuntimeProtocolGuard::new("fixture-run", case.maximum_bytes).expect("valid guard");
        let error = guard.accept_request(&encoded).expect_err(&case.name);
        let actual = match error {
            RuntimeProtocolError::InvalidFormat(_) => "invalid_format",
            RuntimeProtocolError::InvalidNonce => "invalid_nonce",
            RuntimeProtocolError::OutOfOrder { .. } => "out_of_order",
            RuntimeProtocolError::Malformed(_) => "malformed",
            RuntimeProtocolError::PayloadTooLarge { .. } => "payload_too_large",
            RuntimeProtocolError::EmptyNonce | RuntimeProtocolError::SequenceOverflow => {
                panic!("{} produced a guard-only error", case.name)
            }
        };
        assert_eq!(actual, case.expected, "{}", case.name);
    }
}

#[test]
fn arbitrary_protocol_bytes_are_typed_rejection_or_valid_bounded_input() {
    // Fixed generator: this is reproducible in CI and deliberately includes empty, truncated,
    // non-UTF8 and near-cap payloads. A richer coverage-guided lane can build on the same rule.
    let mut state = 0xB11F_F1A5_CAFE_0042_u64;
    for case_index in 0..2_048_usize {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let length = (state as usize) % 513;
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            bytes.push((state >> 56) as u8);
        }
        let mut guard = RuntimeProtocolGuard::new("fixture-run", 512).expect("valid guard");
        match guard.accept_request(&bytes) {
            Ok(envelope) => {
                assert_eq!(envelope.session_nonce, "fixture-run", "case {case_index}");
                assert_eq!(envelope.sequence, 0, "case {case_index}");
            }
            Err(
                RuntimeProtocolError::PayloadTooLarge { .. }
                | RuntimeProtocolError::Malformed(_)
                | RuntimeProtocolError::InvalidFormat(_)
                | RuntimeProtocolError::InvalidNonce
                | RuntimeProtocolError::OutOfOrder { .. },
            ) => {}
            Err(RuntimeProtocolError::EmptyNonce | RuntimeProtocolError::SequenceOverflow) => {
                panic!("case {case_index} reached a guard-construction-only failure")
            }
        }
    }
}
