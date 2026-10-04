// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded verification for the private gateway profile, not DID discovery or
//! native Spaces. Never feed untrusted CAR/MST bytes to Atrium's trusted loaders.
use atrium_api::types::{
    Unknown,
    string::{Did, Nsid, RecordKey, Tid},
};
use atrium_crypto::{did, verify::verify_signature};
use atrium_repo::{Cid, blockstore::MemoryBlockStore, mst::Tree};
use mem_atproto::{
    EncodedAtRecord, addressed_expertise::AddressedExpertiseWire, publication::PublicationWire,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeMap, BTreeSet};

mod car;
mod detached;
pub use detached::{DetachedSigner, DetachedVerifier};
mod mst_store;
pub use car::MAX_CAR_BYTES;
use car::{block, parse_car};
use mst_store::CanonicalMstStore;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures;
#[cfg(test)]
mod tests;

const MAX_METADATA_BYTES: usize = 4096;
const MAX_RECORDS: usize = 8192;
const MAX_DEPTH: usize = 64;
// AT repository keys allow a 317-byte NSID and a 512-byte rkey.
const MAX_KEY_BYTES: usize = 830;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("invalid enrolled repository identity or signing key")]
    Enrollment,
    #[error("repository proof exceeds private-profile resource limits")]
    Limit,
    #[error("invalid CAR framing")]
    Car,
    #[error("invalid CAR header")]
    Header,
    #[error("unsupported block CID or invalid block hash")]
    Block,
    #[error("required repository block is absent")]
    MissingBlock,
    #[error("invalid or noncanonical signed commit")]
    Commit,
    #[error("repository signature does not match the enrolled identity/key")]
    Signature,
    #[error("incomplete or noncanonical repository tree")]
    Tree,
    #[error("expected publication is not committed at its exact key")]
    Record,
}
type Result<T> = std::result::Result<T, VerifyError>;

/// A binding enrolled by a trusted administrator, not a self-asserted DID/key.
/// No discovery, URL dereferencing, TOFU, rotation or historical-key fallback.
#[derive(Clone)]
pub struct PinnedRepository {
    repository: Did,
    signing_key: String,
}

/// Proof of inclusion in this signed snapshot only. It proves neither factual
/// truth nor current authorization/freshness/append-only history. Durable audit
/// receipts and receiver verification are provided by mem-federation-runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedRepository {
    pub repository: String,
    pub signing_key: String,
    pub commit_cid: String,
    pub revision: String,
    pub record_cid: String,
}
/// A newer signed, canonical repository snapshot in which the exact addressed
/// key is absent. It does not prove deletion from PDS backups or past recipients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedDeletion {
    pub repository: String,
    pub signing_key: String,
    pub commit_cid: String,
    pub revision: String,
    pub prior_record_cid: String,
}

fn validate_signing_key(signing_key: &str) -> Result<()> {
    // Guard Atrium 0.1.3's parse_multikey slice before invoking it. Limit the
    // encoded key too: decoding an arbitrarily long administrator input is
    // unnecessary. Only canonical compressed base58btc keys are supported.
    if signing_key.len() > 128 {
        return Err(VerifyError::Enrollment);
    }
    let encoded = signing_key
        .strip_prefix("did:key:")
        .ok_or(VerifyError::Enrollment)?;
    let (base, decoded) = multibase::decode(encoded).map_err(|_| VerifyError::Enrollment)?;
    if base != multibase::Base::Base58Btc || decoded.len() != 35 {
        return Err(VerifyError::Enrollment);
    }
    let (algorithm, key) = did::parse_did_key(signing_key).map_err(|_| VerifyError::Enrollment)?;
    if did::format_did_key(algorithm, &key).map_err(|_| VerifyError::Enrollment)? != signing_key {
        return Err(VerifyError::Enrollment);
    }
    Ok(())
}
impl PinnedRepository {
    pub fn new(repository: String, signing_key: String) -> Result<Self> {
        let repository = Did::new(repository).map_err(|_| VerifyError::Enrollment)?;
        validate_signing_key(&signing_key)?;
        Ok(Self {
            repository,
            signing_key,
        })
    }

    /// CPU-bound, bounded full-snapshot verification. Services must run this on
    /// a bounded blocking worker, not their async I/O executor. `now_micros` is
    /// trusted wall time; the five-minute future tolerance is not replay defense.
    pub fn verify_full_car(
        &self,
        bytes: &[u8],
        wire: &PublicationWire,
        now_micros: u64,
    ) -> Result<VerifiedRepository> {
        let expected = wire.record().map_err(|_| VerifyError::Record)?;
        let cid = wire.cid().map_err(|_| VerifyError::Record)?;
        self.verify_full_car_record(bytes, &expected, &cid, wire.raw(), now_micros)
    }

    /// The addressed collection uses the same pinned signature, canonical MST
    /// and exact record-byte checks as ordinary publication. This does not
    /// authorize a recipient; the gateway must do that before returning bytes.
    pub fn verify_full_car_addressed(
        &self,
        bytes: &[u8],
        wire: &AddressedExpertiseWire,
        now_micros: u64,
    ) -> Result<VerifiedRepository> {
        let expected = wire.record().map_err(|_| VerifyError::Record)?;
        let cid = wire.cid().map_err(|_| VerifyError::Record)?;
        self.verify_full_car_record(bytes, &expected, &cid, wire.raw(), now_micros)
    }

    pub fn verify_full_car_absent_addressed(
        &self,
        bytes: &[u8],
        wire: &AddressedExpertiseWire,
        prior_revision: &str,
        now_micros: u64,
    ) -> Result<VerifiedDeletion> {
        let expected = wire.record().map_err(|_| VerifyError::Record)?;
        let prior_record_cid = wire.cid().map_err(|_| VerifyError::Record)?;
        let (root, revision, entries, _) = self.verified_snapshot(bytes, now_micros)?;
        let key = format!(
            "{}/{}",
            expected.collection.as_ref(),
            expected.rkey.as_ref()
        );
        // A signed snapshot from before publication is not proof of deletion.
        if entries.contains_key(&key) || revision.as_str() <= prior_revision {
            return Err(VerifyError::Record);
        }
        Ok(VerifiedDeletion {
            repository: self.repository.as_ref().to_owned(),
            signing_key: self.signing_key.clone(),
            commit_cid: root.to_string(),
            revision,
            prior_record_cid,
        })
    }

    fn verify_full_car_record(
        &self,
        bytes: &[u8],
        expected: &EncodedAtRecord,
        expected_cid: &str,
        raw: &Unknown,
        now_micros: u64,
    ) -> Result<VerifiedRepository> {
        let (root, revision, entries, blocks) = self.verified_snapshot(bytes, now_micros)?;
        let key = format!(
            "{}/{}",
            expected.collection.as_ref(),
            expected.rkey.as_ref()
        );
        let cid: Cid = expected_cid.parse().map_err(|_| VerifyError::Record)?;
        let value = serde_ipld_dagcbor::to_vec(raw).map_err(|_| VerifyError::Record)?;
        if entries.get(&key) != Some(&cid) || block(&blocks, &cid)? != value {
            return Err(VerifyError::Record);
        }
        Ok(VerifiedRepository {
            repository: self.repository.as_ref().to_owned(),
            signing_key: self.signing_key.clone(),
            commit_cid: root.to_string(),
            revision,
            record_cid: cid.to_string(),
        })
    }

    fn verified_snapshot<'a>(
        &self,
        bytes: &'a [u8],
        now_micros: u64,
    ) -> Result<(Cid, String, BTreeMap<String, Cid>, car::Blocks<'a>)> {
        let (root, blocks) = parse_car(bytes)?;
        let commit_bytes = block(&blocks, &root)?;
        if commit_bytes.len() > MAX_METADATA_BYTES {
            return Err(VerifyError::Limit);
        }
        let commit: SignedCommit = canonical(commit_bytes).map_err(|_| VerifyError::Commit)?;
        if commit.did != self.repository || commit.version != 3 || commit.sig.len() != 64 {
            return Err(VerifyError::Commit);
        }
        // Tid's parser restricts the alphabet and top bit. Decode the timestamp
        // only after that parser has run, without interpreting it as app order.
        let alphabet = b"234567abcdefghijklmnopqrstuvwxyz";
        let mut tid = 0u64;
        for c in commit.rev.as_str().bytes() {
            let digit = alphabet
                .iter()
                .position(|b| *b == c)
                .ok_or(VerifyError::Commit)?;
            tid = tid
                .checked_mul(32)
                .and_then(|n| n.checked_add(digit as u64))
                .ok_or(VerifyError::Commit)?;
        }
        if (tid >> 10) > now_micros.saturating_add(300_000_000) {
            return Err(VerifyError::Commit);
        }
        let unsigned =
            serde_ipld_dagcbor::to_vec(&commit.unsigned()).map_err(|_| VerifyError::Commit)?;
        verify_signature(&self.signing_key, &unsigned, &commit.sig)
            .map_err(|_| VerifyError::Signature)?;
        let entries = parse_tree(commit.data, &blocks)?;
        // Never traverse the received tree with Atrium. Reconstruct from checked
        // entries in fresh memory; root equality proves ordering, layer placement,
        // canonical prefix compression and topology of the received tree.
        let rebuilt = futures::executor::block_on(async {
            let mut tree = Tree::create(CanonicalMstStore(MemoryBlockStore::new()))
                .await
                .map_err(|_| VerifyError::Tree)?;
            for (key, cid) in &entries {
                tree.add(key, *cid).await.map_err(|_| VerifyError::Tree)?;
            }
            Ok::<_, VerifyError>(tree.root())
        })?;
        if rebuilt != commit.data {
            return Err(VerifyError::Tree);
        }
        Ok((root, commit.rev.as_str().to_owned(), entries, blocks))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedCommit {
    // Canonical CBOR field order: encoded key length, then bytes. The pinned
    // serializer sorts maps but NOT structs. Keep the upstream golden tests.
    did: Did,
    rev: Tid,
    #[serde(with = "serde_bytes")]
    sig: Vec<u8>,
    data: Cid,
    prev: Option<Cid>,
    version: i64,
}
#[derive(Serialize)]
struct UnsignedCommit<'a> {
    did: &'a Did,
    rev: &'a Tid,
    data: Cid,
    prev: Option<Cid>,
    version: i64,
}
impl SignedCommit {
    fn unsigned(&self) -> UnsignedCommit<'_> {
        UnsignedCommit {
            did: &self.did,
            version: self.version,
            data: self.data,
            rev: &self.rev,
            prev: self.prev,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    e: Vec<Entry>,
    l: Option<Cid>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    #[serde(with = "serde_bytes")]
    k: Vec<u8>,
    p: usize,
    t: Option<Cid>,
    v: Cid,
}
fn canonical<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> std::result::Result<T, ()> {
    let value: T = serde_ipld_dagcbor::from_slice(bytes).map_err(|_| ())?;
    if serde_ipld_dagcbor::to_vec(&value).map_err(|_| ())? != bytes {
        return Err(());
    }
    Ok(value)
}
fn parse_tree(root: Cid, blocks: &BTreeMap<Cid, &[u8]>) -> Result<BTreeMap<String, Cid>> {
    let mut todo = vec![(root, 0usize)];
    let mut visited = BTreeSet::new();
    let mut entries = BTreeMap::new();
    while let Some((cid, depth)) = todo.pop() {
        if depth > MAX_DEPTH || visited.len() >= MAX_RECORDS {
            return Err(VerifyError::Limit);
        }
        if !visited.insert(cid) {
            return Err(VerifyError::Tree);
        }
        let node: Node = canonical(block(blocks, &cid)?).map_err(|_| VerifyError::Tree)?;
        if let Some(left) = node.l {
            todo.push((left, depth + 1));
        }
        let mut previous = Vec::new();
        for entry in node.e {
            if entries.len() >= MAX_RECORDS {
                return Err(VerifyError::Limit);
            }
            if entry.p > previous.len() || entry.p.saturating_add(entry.k.len()) > MAX_KEY_BYTES {
                return Err(VerifyError::Tree);
            }
            let mut key = previous[..entry.p].to_vec();
            key.extend_from_slice(&entry.k);
            let text = std::str::from_utf8(&key).map_err(|_| VerifyError::Tree)?;
            let (nsid, rkey) = text.split_once('/').ok_or(VerifyError::Tree)?;
            Nsid::new(nsid.to_owned()).map_err(|_| VerifyError::Tree)?;
            RecordKey::new(rkey.to_owned()).map_err(|_| VerifyError::Tree)?;
            if !previous.is_empty() && key <= previous {
                return Err(VerifyError::Tree);
            }
            block(blocks, &entry.v)?;
            if entries.insert(text.to_owned(), entry.v).is_some() {
                return Err(VerifyError::Tree);
            }
            if let Some(right) = entry.t {
                todo.push((right, depth + 1));
            }
            previous = key;
        }
    }
    Ok(entries)
}
