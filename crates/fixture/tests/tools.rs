//! The tools run natively, with the SDK standing in for the host.

use berm_lang::{CallError, test};

#[test]
fn echo_wraps_the_payload() {
    let out = test::call(berm_fixture::berm_tool_echo, br#"{"query":"hi"}"#).unwrap();
    assert_eq!(out, br#"{"echo":{"query":"hi"}}"#);
}

#[test]
fn boom_reports_its_message() {
    let error = test::call(berm_fixture::berm_tool_boom, b"").unwrap_err();
    assert_eq!(error, "boom, as requested");
}

#[test]
fn probe_allocates() {
    assert_eq!(
        test::call(berm_fixture::berm_tool_probe, b"").unwrap(),
        [7, 7]
    );
}

#[test]
fn nest_forwards_what_the_other_harness_answered() {
    test::answer("inner", "echo", Ok(br#"{"echo":"pong"}"#));
    let out = test::call(berm_fixture::berm_tool_nest, br#"{"query":"hi"}"#).unwrap();
    assert_eq!(out, br#"nested:{"echo":"pong"}"#);
    test::forget();
}

/// The distinction the second bit on the wire exists for: a target that never
/// ran reads differently from one that ran and said no.
#[test]
fn nest_tells_a_refusal_from_a_failure() {
    test::answer(
        "inner",
        "echo",
        Err(CallError::Refused(
            "no harness named \"inner\" is deployed".into(),
        )),
    );
    let error = test::call(berm_fixture::berm_tool_nest, b"{}").unwrap_err();
    assert_eq!(error, "refused: no harness named \"inner\" is deployed");

    test::answer(
        "inner",
        "echo",
        Err(CallError::Failed("the target said no".into())),
    );
    let error = test::call(berm_fixture::berm_tool_nest, b"{}").unwrap_err();
    assert_eq!(error, "the target said no");
    test::forget();
}
