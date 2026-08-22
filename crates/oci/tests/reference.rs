//! Parsing a reference, and refusing one that is not a name.
//!
//! The refusals matter as much as the parses. A repository becomes a path — an
//! index holds one file per harness at `{registry}/{repository}.json` — so a
//! name a registry served could otherwise be written outside it.

use berm_oci::Reference;
use std::str::FromStr;

fn parse(text: &str) -> Reference {
    Reference::from_str(text).unwrap_or_else(|error| panic!("{text:?} should parse: {error}"))
}

fn refuse(text: &str) {
    assert!(
        Reference::from_str(text).is_err(),
        "{text:?} should be refused"
    );
}

#[test]
fn splits_registry_repository_and_reference() {
    let reference = parse("ghcr.io/org/example:v1");
    assert_eq!(reference.registry, "ghcr.io");
    assert_eq!(reference.repository, "org/example");
    assert_eq!(reference.reference, "v1");
}

/// A package inside a project is more than two segments deep, and everything
/// after the registry is the repository however many there are.
#[test]
fn a_repository_is_as_deep_as_it_needs_to_be() {
    let reference = parse("ghcr.io/crabtalk/crabtalk/pkg:abc");
    assert_eq!(reference.repository, "crabtalk/crabtalk/pkg");
    assert_eq!(reference.reference, "abc");

    assert_eq!(parse("ghcr.io/a/b/c/d/e:tag").repository, "a/b/c/d/e");
}

#[test]
fn an_absent_tag_is_latest() {
    assert_eq!(parse("ghcr.io/crabtalk/crabtalk/pkg").reference, "latest");
}

/// A digest is held with `@`, and the `:` inside it is not a tag separator.
#[test]
fn a_digest_is_read_whole() {
    let reference = parse("ghcr.io/crabtalk/crabtalk/pkg@sha256:deadbeef");
    assert_eq!(reference.repository, "crabtalk/crabtalk/pkg");
    assert_eq!(reference.reference, "sha256:deadbeef");
}

#[test]
fn a_port_belongs_to_the_registry() {
    let reference = parse("localhost:5000/deep/nested/path:v2");
    assert_eq!(reference.registry, "localhost:5000");
    assert_eq!(reference.repository, "deep/nested/path");
    assert_eq!(reference.reference, "v2");
}

#[test]
fn the_separators_the_grammar_allows() {
    assert_eq!(
        parse("reg.io/a.b/c_d/e__f/g--h:v1").repository,
        "a.b/c_d/e__f/g--h"
    );
    assert_eq!(parse("reg.io/x0/9y:v1").repository, "x0/9y");
}

#[test]
fn what_is_parsed_prints_back_unchanged() {
    for text in [
        "ghcr.io/org/example:v1",
        "ghcr.io/crabtalk/crabtalk/pkg:abc",
        "localhost:5000/deep/nested/path:v2",
        "ghcr.io/crabtalk/crabtalk/pkg@sha256:deadbeef",
    ] {
        assert_eq!(parse(text).to_string(), text);
    }
    // The rewrites: an absent tag is filled in rather than left off, and a
    // name is folded rather than refused.
    assert_eq!(
        parse("ghcr.io/crabtalk/crabtalk/pkg").to_string(),
        "ghcr.io/crabtalk/crabtalk/pkg:latest"
    );
    assert_eq!(
        parse("GHCR.io/Crabtalk/Berm:v1").to_string(),
        "ghcr.io/crabtalk/berm:v1"
    );
}

/// A GitHub org keeps its capitals; a registry will only serve the lowercase
/// form, so folding it is what makes the reference someone typed work.
#[test]
fn a_registry_and_repository_are_folded_to_lowercase() {
    let reference = parse("GHCR.IO/Crabtalk/Crabtalk/Pkg:abc");
    assert_eq!(reference.registry, "ghcr.io");
    assert_eq!(reference.repository, "crabtalk/crabtalk/pkg");
}

/// But not the tag. Its grammar admits uppercase and a registry tells the two
/// apart, so folding one would fetch a different image than was asked for.
#[test]
fn a_tag_keeps_its_case() {
    assert_eq!(parse("ghcr.io/org/example:V1").reference, "V1");
    assert_eq!(
        parse("ghcr.io/Org/Example:ReleaseCandidate").reference,
        "ReleaseCandidate"
    );
    assert_eq!(
        parse("ghcr.io/org/example@sha256:DEADBEEF").reference,
        "sha256:DEADBEEF"
    );
}

/// `..` is the one that would leave the index's directory. It is refused
/// because the grammar has no way to spell it, not by a check for `..`.
#[test]
fn a_name_cannot_climb_out_of_a_directory() {
    refuse("evil.com/../../etc/cron.d/x:v1");
    refuse("evil.com/..:v1");
    refuse("../evil/repo:v1");
    refuse("..:5000/a/b:v1");
}

#[test]
fn a_registry_has_to_look_like_a_hostname() {
    refuse("-bad.com/a/b:v1");
    refuse("reg.io:notaport/a/b:v1");
    refuse("example/a/b:v1");
    refuse("noslash");
}

#[test]
fn a_repository_outside_the_oci_grammar_is_refused() {
    refuse("ghcr.io//example:v1");
    refuse("ghcr.io/org//x:v1");
    refuse("ghcr.io/org/:v1");
    refuse("ghcr.io/-lead/x:v1");
    refuse("ghcr.io/trail-/x:v1");
    refuse("ghcr.io/a___b/x:v1");
    refuse("ghcr.io/.dot/x:v1");
}
