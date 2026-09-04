SPDX-License-Identifier: CC0-1.0

# ATProto Federation Spike Fixtures

This directory vendors the minimal offline fixture subset used by
`crates/mem-atproto`.

Upstream source:
- Repository: `bluesky-social/atproto-interop-tests`
- Commit: `056e5741bb330757205d6b16db5266fffcae937b`
- License: CC0 1.0 Universal

Vendored upstream files:
- `syntax/atidentifier_syntax_valid.txt`
- `syntax/atidentifier_syntax_invalid.txt`
- `syntax/aturi_syntax_valid.txt`
- `syntax/aturi_syntax_invalid.txt`
- `syntax/cid_syntax_valid.txt`
- `syntax/cid_syntax_invalid.txt`
- `syntax/did_syntax_valid.txt`
- `syntax/did_syntax_invalid.txt`
- `syntax/nsid_syntax_valid.txt`
- `syntax/nsid_syntax_invalid.txt`
- `syntax/recordkey_syntax_valid.txt`
- `syntax/recordkey_syntax_invalid.txt`
- `syntax/tid_syntax_valid.txt`
- `syntax/tid_syntax_invalid.txt`
- `mst/example_keys.txt`
- `LICENSE-CC0`

Locally authored files:
- `xrpc/create_record_request.json`
- `xrpc/apply_writes_request.json`
- `xrpc/list_records_request_uri.txt`

The locally authored XRPC goldens are derived from the handwritten DTOs and
Atrium-generated `com.atproto.repo.{createRecord,applyWrites,listRecords}`
request shapes pinned in this spike.
