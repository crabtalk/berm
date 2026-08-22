//! The tools run natively, with the SDK standing in for the host.

use berm_lang::test;

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
