// SPDX-License-Identifier: AGPL-3.0-or-later

pub struct SyntaxFixture {
    pub name: &'static str,
    pub valid: &'static str,
    pub invalid: &'static str,
}

pub const SYNTAX_FIXTURES: &[SyntaxFixture] = &[
    SyntaxFixture {
        name: "atidentifier",
        valid: include_str!(
            "../../../fixtures/federation/atproto/syntax/atidentifier_syntax_valid.txt"
        ),
        invalid: include_str!(
            "../../../fixtures/federation/atproto/syntax/atidentifier_syntax_invalid.txt"
        ),
    },
    SyntaxFixture {
        name: "aturi",
        valid: include_str!("../../../fixtures/federation/atproto/syntax/aturi_syntax_valid.txt"),
        invalid: include_str!(
            "../../../fixtures/federation/atproto/syntax/aturi_syntax_invalid.txt"
        ),
    },
    SyntaxFixture {
        name: "cid",
        valid: include_str!("../../../fixtures/federation/atproto/syntax/cid_syntax_valid.txt"),
        invalid: include_str!("../../../fixtures/federation/atproto/syntax/cid_syntax_invalid.txt"),
    },
    SyntaxFixture {
        name: "did",
        valid: include_str!("../../../fixtures/federation/atproto/syntax/did_syntax_valid.txt"),
        invalid: include_str!("../../../fixtures/federation/atproto/syntax/did_syntax_invalid.txt"),
    },
    SyntaxFixture {
        name: "nsid",
        valid: include_str!("../../../fixtures/federation/atproto/syntax/nsid_syntax_valid.txt"),
        invalid: include_str!(
            "../../../fixtures/federation/atproto/syntax/nsid_syntax_invalid.txt"
        ),
    },
    SyntaxFixture {
        name: "recordkey",
        valid: include_str!(
            "../../../fixtures/federation/atproto/syntax/recordkey_syntax_valid.txt"
        ),
        invalid: include_str!(
            "../../../fixtures/federation/atproto/syntax/recordkey_syntax_invalid.txt"
        ),
    },
    SyntaxFixture {
        name: "tid",
        valid: include_str!("../../../fixtures/federation/atproto/syntax/tid_syntax_valid.txt"),
        invalid: include_str!("../../../fixtures/federation/atproto/syntax/tid_syntax_invalid.txt"),
    },
];

pub const MST_EXAMPLE_KEYS: &str =
    include_str!("../../../fixtures/federation/atproto/mst/example_keys.txt");
pub const CREATE_RECORD_REQUEST_GOLDEN: &str =
    include_str!("../../../fixtures/federation/atproto/xrpc/create_record_request.json");
pub const APPLY_WRITES_REQUEST_GOLDEN: &str =
    include_str!("../../../fixtures/federation/atproto/xrpc/apply_writes_request.json");
pub const LIST_RECORDS_REQUEST_URI_GOLDEN: &str =
    include_str!("../../../fixtures/federation/atproto/xrpc/list_records_request_uri.txt");
pub const FIXTURE_PROVENANCE: &str = include_str!("../../../fixtures/federation/atproto/README.md");
pub const FIXTURE_LICENSE_CC0: &str =
    include_str!("../../../fixtures/federation/atproto/LICENSE-CC0");

pub fn fixture_lines(contents: &str) -> impl Iterator<Item = &str> {
    contents.lines().filter(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    })
}
