//! serde adapter that ferries Vec<u8> as base64 strings in JSON.
//!
//! Use as `#[serde(with = "crate::b64")]` on fields the client should see as
//! base64-encoded bytes.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&STANDARD.encode(bytes))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let s = <String>::deserialize(d)?;
    STANDARD.decode(s).map_err(D::Error::custom)
}

pub mod option {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => s.serialize_str(&STANDARD.encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let s = <Option<String>>::deserialize(d)?;
        match s {
            Some(s) => Ok(Some(STANDARD.decode(s).map_err(D::Error::custom)?)),
            None => Ok(None),
        }
    }
}

/// Hex-encoded bytes. Used for content-addressed identifiers (sha256), not
/// arbitrary blobs.
pub mod hex_option {
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => s.serialize_str(&hex::encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let s = <Option<String>>::deserialize(d)?;
        match s {
            Some(s) => Ok(Some(hex::decode(s).map_err(D::Error::custom)?)),
            None => Ok(None),
        }
    }
}
