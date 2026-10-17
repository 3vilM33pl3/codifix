// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::MemAtprotoError;
use atrium_api::types::string::{AtIdentifier, Nsid, RecordKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtUri {
    raw: String,
    authority: AtIdentifier,
    collection: Option<Nsid>,
    rkey: Option<RecordKey>,
}

impl AtUri {
    pub fn authority(&self) -> &AtIdentifier {
        &self.authority
    }

    pub fn collection(&self) -> Option<&Nsid> {
        self.collection.as_ref()
    }

    pub fn rkey(&self) -> Option<&RecordKey> {
        self.rkey.as_ref()
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for AtUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AtUri {
    type Err = MemAtprotoError;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        if src.is_empty() {
            return Err(MemAtprotoError::AtUri("empty AT URI"));
        }
        if src.len() > 8_192 {
            return Err(MemAtprotoError::AtUri("AT URI exceeds 8192 bytes"));
        }
        if src.contains(char::is_whitespace) {
            return Err(MemAtprotoError::AtUri("AT URI contains whitespace"));
        }
        if src.contains('#') || src.contains('?') {
            return Err(MemAtprotoError::AtUri(
                "AT URI must not contain fragments or queries",
            ));
        }
        if src.ends_with('/') {
            return Err(MemAtprotoError::AtUri("AT URI must not end with '/'"));
        }
        let rest = src
            .strip_prefix("at://")
            .ok_or(MemAtprotoError::AtUri("AT URI must start with at://"))?;
        if rest.is_empty() {
            return Err(MemAtprotoError::AtUri("AT URI authority is missing"));
        }
        let segments: Vec<&str> = rest.split('/').collect();
        if segments.iter().any(|segment| segment.is_empty()) {
            return Err(MemAtprotoError::AtUri(
                "AT URI contains an empty path segment",
            ));
        }
        if segments.len() > 3 {
            return Err(MemAtprotoError::AtUri("AT URI has too many path segments"));
        }
        let authority = segments[0]
            .parse::<AtIdentifier>()
            .map_err(|_| MemAtprotoError::AtUri("invalid AT URI authority"))?;
        let collection = if let Some(segment) = segments.get(1) {
            Some(
                segment
                    .parse::<Nsid>()
                    .map_err(|_| MemAtprotoError::AtUri("invalid AT URI collection"))?,
            )
        } else {
            None
        };
        let rkey = if let Some(segment) = segments.get(2) {
            Some(
                segment
                    .parse::<RecordKey>()
                    .map_err(|_| MemAtprotoError::AtUri("invalid AT URI record key"))?,
            )
        } else {
            None
        };
        Ok(Self {
            raw: src.to_owned(),
            authority,
            collection,
            rkey,
        })
    }
}

impl Serialize for AtUri {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AtUri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}
