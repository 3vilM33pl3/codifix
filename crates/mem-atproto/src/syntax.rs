// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(test)]
mod tests {
    use crate::{AtUri, SYNTAX_FIXTURES, fixture_lines};
    use atrium_api::types::string::{AtIdentifier, Cid, Did, Nsid, RecordKey, Tid};
    use std::str::FromStr;

    fn fixture(name: &str) -> &'static crate::SyntaxFixture {
        SYNTAX_FIXTURES
            .iter()
            .find(|fixture| fixture.name == name)
            .unwrap()
    }

    fn assert_valid<T>(name: &str)
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        for sample in fixture_lines(fixture(name).valid) {
            sample
                .parse::<T>()
                .unwrap_or_else(|err| panic!("{name} should accept `{sample}`: {err}"));
        }
    }

    fn assert_invalid<T>(name: &str)
    where
        T: FromStr,
        T::Err: std::fmt::Display,
    {
        for sample in fixture_lines(fixture(name).invalid) {
            assert!(
                sample.parse::<T>().is_err(),
                "{name} should reject `{sample}`"
            );
        }
    }

    #[test]
    fn atrium_string_types_match_vendored_syntax_fixtures() {
        assert_valid::<AtIdentifier>("atidentifier");
        assert_invalid::<AtIdentifier>("atidentifier");
        for sample in [
            "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            "zdj7WWeQ43G6JJvLWQWZpyHuAMq6uYWRjkBXFad11vE2LHhQ7",
            "bafybeie5gq4jxvzmsym6hjlwxej4rwdoxt7wadqvmmwbqi7r27fclha2va",
        ] {
            sample
                .parse::<Cid>()
                .unwrap_or_else(|err| panic!("cid should accept `{sample}`: {err}"));
        }
        for sample in [
            "example.com",
            "https://example.com",
            "cid:bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ".",
            "12345",
            "noop",
        ] {
            assert!(
                sample.parse::<Cid>().is_err(),
                "cid should reject `{sample}`"
            );
        }
        assert_valid::<Did>("did");
        assert_invalid::<Did>("did");
        assert_valid::<Nsid>("nsid");
        assert_invalid::<Nsid>("nsid");
        assert_valid::<RecordKey>("recordkey");
        assert_invalid::<RecordKey>("recordkey");
        assert_valid::<Tid>("tid");
        assert_invalid::<Tid>("tid");
    }

    #[test]
    fn local_at_uri_parser_matches_vendored_syntax_fixtures() {
        assert_valid::<AtUri>("aturi");
        assert_invalid::<AtUri>("aturi");
    }

    #[test]
    fn vendored_fixture_set_is_provenanced() {
        assert!(crate::FIXTURE_PROVENANCE.contains("056e5741bb330757205d6b16db5266fffcae937b"));
        assert!(crate::FIXTURE_LICENSE_CC0.contains("CC0 1.0 Universal"));
        assert!(fixture_lines(crate::MST_EXAMPLE_KEYS).count() > 100);
        assert!(crate::CREATE_RECORD_REQUEST_GOLDEN.contains("\"collection\""));
        assert!(fixture("cid").valid.contains("mBcDxtdWx0aWhhc2g+"));
        assert!(fixture("cid").valid.contains("z7x3CtScH765HvShXT"));
        assert!(
            fixture("cid")
                .invalid
                .contains("QmbWqxBEKC3P8tqsKc98xmWNzrzDtRLMiMPL8wBuTGsMnR")
        );
    }
}
