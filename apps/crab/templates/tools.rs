//! The tools run natively, with the SDK standing in for the host.

use berm_lang::test;

#[test]
fn echo_answers_its_argument() {
    let out = test::call(__CRATE__::berm_tool_echo, br#"{"query":"hi"}"#).unwrap();
    assert_eq!(out, br#"{"query":"hi"}"#);
}
