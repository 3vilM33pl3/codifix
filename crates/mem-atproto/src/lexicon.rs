// SPDX-License-Identifier: AGPL-3.0-or-later

pub struct LexiconSchema {
    pub nsid: &'static str,
    pub json: &'static str,
}

pub const MEMORY_VERSION_NSID: &str = "dev.memory-layer.unspecced.memoryVersion";
pub const CANONICAL_STATE_NSID: &str = "dev.memory-layer.unspecced.canonicalState";
pub const EVIDENCE_NSID: &str = "dev.memory-layer.unspecced.evidence";
pub const RELATION_NSID: &str = "dev.memory-layer.unspecced.relation";
pub const MEMBERSHIP_NSID: &str = "dev.memory-layer.unspecced.membership";
pub const REVOCATION_NSID: &str = "dev.memory-layer.unspecced.revocation";

pub const UNSPECCED_LEXICONS: &[LexiconSchema] = &[
    LexiconSchema {
        nsid: MEMORY_VERSION_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.memoryVersion.json"),
    },
    LexiconSchema {
        nsid: CANONICAL_STATE_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.canonicalState.json"),
    },
    LexiconSchema {
        nsid: EVIDENCE_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.evidence.json"),
    },
    LexiconSchema {
        nsid: RELATION_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.relation.json"),
    },
    LexiconSchema {
        nsid: MEMBERSHIP_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.membership.json"),
    },
    LexiconSchema {
        nsid: REVOCATION_NSID,
        json: include_str!("../lexicons/dev.memory-layer.unspecced.revocation.json"),
    },
];
