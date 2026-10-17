// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::MemAtprotoError;
use atrium_repo::{
    Cid, Multihash,
    blockstore::{DAG_CBOR, SHA2_256},
};
use serde::Serialize;
use sha2::Digest;

pub fn dag_cbor_cid<T>(value: &T) -> Result<Cid, MemAtprotoError>
where
    T: Serialize,
{
    let bytes = serde_ipld_dagcbor::to_vec(value)
        .map_err(|err| MemAtprotoError::message(err.to_string()))?;
    let digest = sha2::Sha256::digest(&bytes);
    let multihash = Multihash::wrap(SHA2_256, digest.as_slice())
        .map_err(|err| MemAtprotoError::message(err.to_string()))?;
    Ok(Cid::new_v1(DAG_CBOR, multihash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MEMORY_VERSION_NSID, MST_EXAMPLE_KEYS, fixture_lines};
    use atrium_repo::{
        blockstore::{AsyncBlockStoreRead, AsyncBlockStoreWrite, MemoryBlockStore},
        mst::Tree,
    };
    use futures::{TryStreamExt, executor::block_on, pin_mut, task::noop_waker};
    use std::{
        future::Future,
        sync::{Arc, Mutex},
        task::{Context, Poll},
    };

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeafRecord<'a> {
        #[serde(rename = "$type")]
        record_type: &'static str,
        key: &'a str,
    }

    #[derive(Clone, Default)]
    struct SharedMemoryBlockStore {
        inner: Arc<Mutex<MemoryBlockStore>>,
    }

    impl SharedMemoryBlockStore {
        fn new() -> Self {
            Self {
                inner: Arc::new(Mutex::new(MemoryBlockStore::new())),
            }
        }

        fn contains(&self, cid: Cid) -> bool {
            match self.inner.lock() {
                Ok(guard) => guard.contains(cid),
                Err(poisoned) => poisoned.into_inner().contains(cid),
            }
        }

        fn poll_immediate<T>(future: impl Future<Output = T>) -> T {
            pin_mut!(future);
            let waker = noop_waker();
            let mut context = Context::from_waker(&waker);
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("memory blockstore operations should be immediately ready"),
            }
        }
    }

    impl AsyncBlockStoreRead for SharedMemoryBlockStore {
        fn read_block_into(
            &mut self,
            cid: Cid,
            contents: &mut Vec<u8>,
        ) -> impl std::future::Future<Output = Result<(), atrium_repo::blockstore::Error>> + Send
        {
            let result = {
                let mut guard = match self.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                Self::poll_immediate(guard.read_block_into(cid, contents))
            };
            std::future::ready(result)
        }
    }

    impl AsyncBlockStoreWrite for SharedMemoryBlockStore {
        fn write_block(
            &mut self,
            codec: u64,
            hash: u64,
            contents: &[u8],
        ) -> impl std::future::Future<Output = Result<Cid, atrium_repo::blockstore::Error>> + Send
        {
            let result = {
                let mut guard = match self.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                Self::poll_immediate(guard.write_block(codec, hash, contents))
            };
            std::future::ready(result)
        }
    }

    async fn build_tree(keys: &[&str]) -> Result<(Cid, Vec<(String, Cid)>), MemAtprotoError> {
        let mut storage = SharedMemoryBlockStore::new();
        let mut tree = Tree::create(storage.clone())
            .await
            .map_err(|err| MemAtprotoError::message(err.to_string()))?;
        for key in keys {
            let bytes = serde_ipld_dagcbor::to_vec(&LeafRecord {
                record_type: MEMORY_VERSION_NSID,
                key,
            })
            .map_err(|err| MemAtprotoError::message(err.to_string()))?;
            let cid = storage
                .write_block(DAG_CBOR, SHA2_256, &bytes)
                .await
                .map_err(|err| MemAtprotoError::message(err.to_string()))?;
            assert!(storage.contains(cid));
            tree.add(key, cid)
                .await
                .map_err(|err| MemAtprotoError::message(err.to_string()))?;
        }
        let entries = tree
            .entries()
            .try_collect::<Vec<_>>()
            .await
            .map_err(|err| MemAtprotoError::message(err.to_string()))?;
        Ok((tree.root(), entries))
    }

    #[test]
    fn forward_and_reverse_insertion_produce_the_same_root_and_ordered_entries() {
        block_on(async {
            let keys = fixture_lines(MST_EXAMPLE_KEYS).collect::<Vec<_>>();
            let (forward_root, forward_entries) = build_tree(&keys).await.unwrap();
            let reversed_keys = keys.iter().copied().rev().collect::<Vec<_>>();
            let (reverse_root, reverse_entries) = build_tree(&reversed_keys).await.unwrap();

            assert_eq!(forward_root, reverse_root);
            assert_eq!(forward_entries, reverse_entries);

            let ordered_keys = forward_entries
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            let mut expected = keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
            expected.sort();
            assert_eq!(ordered_keys, expected);
        });
    }
}
