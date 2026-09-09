// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic signing material, compiled only for tests / explicit test-fixtures.
//! Never enroll these public, deterministic test keys in a deployment.
#![allow(clippy::unwrap_used)]
use super::*;
use atrium_api::types::{LimitedU32, Unknown};
use atrium_crypto::keypair::{Did as _, P256Keypair, Secp256k1Keypair};
use atrium_repo::blockstore::{AsyncBlockStoreRead, AsyncBlockStoreWrite, DAG_CBOR, SHA2_256};
use futures::TryStreamExt;

#[derive(Clone, Copy)]
pub enum FixtureKey {
    K256,
    P256,
}
impl FixtureKey {
    pub fn did_key(self) -> String {
        match self {
            Self::K256 => Secp256k1Keypair::import(&[1; 32]).unwrap().did(),
            Self::P256 => P256Keypair::import(&[2; 32]).unwrap().did(),
        }
    }
    pub(crate) fn sign(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::K256 => Secp256k1Keypair::import(&[1; 32])
                .unwrap()
                .sign(bytes)
                .unwrap(),
            Self::P256 => P256Keypair::import(&[2; 32]).unwrap().sign(bytes).unwrap(),
        }
    }
}

/// Build a complete, genuinely signed synthetic repository. Each entry is a
/// `collection/rkey` and raw Atrium record; the production verifier has no fake
/// or unsigned mode. Real upstream PDS coverage provides independent encoding.
pub fn signed_repo(
    repository: &str,
    records: &BTreeMap<String, Unknown>,
    key: FixtureKey,
) -> Vec<u8> {
    futures::executor::block_on(async {
        let mut store = MemoryBlockStore::new();
        let mut record_cids = Vec::new();
        for (name, value) in records {
            let bytes = serde_ipld_dagcbor::to_vec(value).unwrap();
            let cid = store.write_block(DAG_CBOR, SHA2_256, &bytes).await.unwrap();
            record_cids.push((name, cid));
        }
        let (root, mut cids) = {
            let mut tree = Tree::create(CanonicalMstStore(&mut store)).await.unwrap();
            for (name, cid) in &record_cids {
                tree.add(name, *cid).await.unwrap();
            }
            (
                tree.root(),
                tree.export().try_collect::<Vec<_>>().await.unwrap(),
            )
        };
        cids.extend(record_cids.into_iter().map(|(_, cid)| cid));
        let mut commit = SignedCommit {
            did: Did::new(repository.into()).unwrap(),
            version: 3,
            data: root,
            rev: Tid::now(LimitedU32::MIN),
            prev: None,
            sig: Vec::new(),
        };
        commit.sig = key.sign(&serde_ipld_dagcbor::to_vec(&commit.unsigned()).unwrap());
        let encoded = serde_ipld_dagcbor::to_vec(&commit).unwrap();
        let commit_cid = store
            .write_block(DAG_CBOR, SHA2_256, &encoded)
            .await
            .unwrap();
        cids.push(commit_cid);
        let mut blocks = BTreeMap::new();
        for cid in cids {
            blocks.insert(cid, store.read_block(cid).await.unwrap());
        }
        pack(commit_cid, &blocks)
    })
}
pub(crate) fn pack(root: Cid, blocks: &BTreeMap<Cid, Vec<u8>>) -> Vec<u8> {
    fn append(out: &mut Vec<u8>, bytes: &[u8]) {
        let mut length = bytes.len();
        while length >= 128 {
            out.push((length as u8 & 127) | 128);
            length >>= 7;
        }
        out.push(length as u8);
        out.extend_from_slice(bytes);
    }
    let mut out = Vec::new();
    append(
        &mut out,
        &serde_ipld_dagcbor::to_vec(&car::Header {
            version: 1,
            roots: vec![root],
        })
        .unwrap(),
    );
    for (cid, bytes) in blocks {
        let mut body = cid.to_bytes();
        body.extend_from_slice(bytes);
        append(&mut out, &body);
    }
    out
}
