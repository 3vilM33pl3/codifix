// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::Node;
use atrium_repo::{
    Cid,
    blockstore::{AsyncBlockStoreRead, AsyncBlockStoreWrite, Error},
};

/// Normalize only LOCALLY GENERATED Atrium MST nodes, before calculating their
/// CID. Atrium 0.1.8 declares node/entry fields in noncanonical order, and pinned
/// serde_ipld_dagcbor 0.6 does not sort struct fields. Untrusted received blocks
/// are never normalized to make them acceptable: their canonical bytes/hash and
/// complete tree must match this independently reconstructed canonical root.
pub(crate) struct CanonicalMstStore<S>(pub S);
impl<S: AsyncBlockStoreRead> AsyncBlockStoreRead for CanonicalMstStore<S> {
    async fn read_block_into(&mut self, cid: Cid, contents: &mut Vec<u8>) -> Result<(), Error> {
        self.0.read_block_into(cid, contents).await
    }
}
impl<S: AsyncBlockStoreWrite> AsyncBlockStoreWrite for CanonicalMstStore<S> {
    async fn write_block(&mut self, codec: u64, hash: u64, contents: &[u8]) -> Result<Cid, Error> {
        let node: Node =
            serde_ipld_dagcbor::from_slice(contents).map_err(|e| Error::Other(Box::new(e)))?;
        let canonical = serde_ipld_dagcbor::to_vec(&node).map_err(|e| Error::Other(Box::new(e)))?;
        self.0.write_block(codec, hash, &canonical).await
    }
}
