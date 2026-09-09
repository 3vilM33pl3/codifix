// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{Result, VerifyError, canonical};
use atrium_repo::{
    Cid,
    blockstore::{DAG_CBOR, SHA2_256},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor};

pub const MAX_CAR_BYTES: usize = 16 * 1024 * 1024;
const MAX_BLOCK_BYTES: usize = 1024 * 1024;
const MAX_BLOCKS: usize = 32768;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Header {
    pub version: u64,
    pub roots: Vec<Cid>,
}

// Decode lengths before allocation, reject overflow/non-minimal varints, and
// only return slices of the already bounded input. No CarStore::open on input.
fn frame<'a>(input: &mut &'a [u8], limit: usize) -> Result<&'a [u8]> {
    let mut size = 0u64;
    let mut count = 0;
    loop {
        let (&byte, rest) = input.split_first().ok_or(VerifyError::Car)?;
        *input = rest;
        if count == 9 && byte > 1 {
            return Err(VerifyError::Car);
        }
        size |= u64::from(byte & 127) << (7 * count);
        count += 1;
        if byte & 128 == 0 {
            if count > 1 && byte == 0 {
                return Err(VerifyError::Car);
            }
            break;
        }
        if count == 10 {
            return Err(VerifyError::Car);
        }
    }
    if size > limit as u64 {
        return Err(VerifyError::Limit);
    }
    let size = usize::try_from(size).map_err(|_| VerifyError::Limit)?;
    if size == 0 || size > input.len() {
        return Err(VerifyError::Car);
    }
    let (bytes, rest) = input.split_at(size);
    *input = rest;
    Ok(bytes)
}
pub(crate) type Blocks<'a> = BTreeMap<Cid, &'a [u8]>;
pub(crate) fn parse_car(mut bytes: &[u8]) -> Result<(Cid, Blocks<'_>)> {
    if bytes.len() > MAX_CAR_BYTES {
        return Err(VerifyError::Limit);
    }
    let header: Header = canonical(frame(&mut bytes, 4096)?).map_err(|_| VerifyError::Car)?;
    // Deliberately a single-root full snapshot, not an incremental CAR/stream.
    if header.version != 1 || header.roots.len() != 1 {
        return Err(VerifyError::Car);
    }
    let mut blocks = BTreeMap::new();
    let mut count = 0;
    while !bytes.is_empty() {
        count += 1;
        if count > MAX_BLOCKS {
            return Err(VerifyError::Limit);
        }
        let body = frame(&mut bytes, MAX_BLOCK_BYTES)?;
        let mut cursor = Cursor::new(body);
        let cid = Cid::read_bytes(&mut cursor).map_err(|_| VerifyError::Car)?;
        let offset = cursor.position() as usize;
        // Equality with re-encoding also rules out noncanonical CID varints.
        if cid.codec() != DAG_CBOR
            || cid.hash().code() != SHA2_256
            || cid.hash().size() != 32
            || cid.to_bytes() != body[..offset]
        {
            return Err(VerifyError::Car);
        }
        let value = &body[offset..];
        if value.is_empty() || Sha256::digest(value).as_slice() != cid.hash().digest() {
            return Err(VerifyError::Car);
        }
        blocks.insert(cid, value);
    }
    block(&blocks, &header.roots[0])?;
    Ok((header.roots[0], blocks))
}
pub(crate) fn block<'a>(blocks: &Blocks<'a>, cid: &Cid) -> Result<&'a [u8]> {
    blocks.get(cid).copied().ok_or(VerifyError::Car)
}
