//! The serialized Proof is already the compressed wire proof: each non-top
//! opened bundle omits its deterministic parent-derived coordinate.
use crate::params::{Error, Result};
use serde::{Serialize, de::DeserializeOwned};
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    postcard::to_allocvec(value).map_err(|e| Error::Codec(e.to_string()))
}
/// Size gate + strict reserialization reject trailing bytes and noncanonical framing.
/// Callers must choose a protocol-derived size limit before parsing untrusted input.
pub fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8], max_bytes: usize) -> Result<T> {
    if bytes.len() > max_bytes {
        return Err(Error::Codec("wire length limit".into()));
    }
    let (value, remaining) =
        postcard::take_from_bytes::<T>(bytes).map_err(|e| Error::Codec(e.to_string()))?;
    if !remaining.is_empty() || encode(&value)? != bytes {
        return Err(Error::Codec(
            "noncanonical framing or trailing bytes".into(),
        ));
    }
    Ok(value)
}
