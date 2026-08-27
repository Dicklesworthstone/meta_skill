// We implemented the duplicate key checks directly inline in the decode impls, but this module might be needed in the future.
#![allow(dead_code)]
#[cfg(feature = "alloc")]
use crate::alloc::vec::Vec;

/// A struct that helps validating deterministic map/set decoding.
/// It re-encodes each key and ensures keys appear in strictly increasing
/// bytewise order (no duplicates, no out-of-order).
#[cfg(feature = "alloc")]
pub struct DeterministicValidator {
    previous_key: Option<Vec<u8>>,
}

#[cfg(feature = "alloc")]
impl DeterministicValidator {
    /// Create a new deterministic validator.
    #[inline(always)]
    pub const fn new() -> Self {
        Self { previous_key: None }
    }

    /// Validate that the given key bytes are strictly greater than the previous key.
    /// Call this with the already-encoded bytes of each key, in order.
    #[inline(always)]
    pub fn validate_key_bytes(
        &mut self,
        key_bytes: Vec<u8>,
    ) -> Result<(), crate::error::DecodeError> {
        if let Some(prev) = &self.previous_key {
            match key_bytes.as_slice().cmp(prev.as_slice()) {
                | core::cmp::Ordering::Less => {
                    return crate::error::cold_decode_error_invalid_map_order();
                },
                | core::cmp::Ordering::Equal => {
                    return crate::error::cold_decode_error_duplicate_map_key();
                },
                | core::cmp::Ordering::Greater => {},
            }
        }
        self.previous_key = Some(key_bytes);
        Ok(())
    }

    /// Re-encode a key value into bytes using the given config, then validate ordering.
    #[inline(always)]
    pub fn validate_key<C: crate::config::Config, K: crate::enc::Encode>(
        &mut self,
        config: &C,
        key: &K,
    ) -> Result<(), crate::error::DecodeError> {
        let mut key_bytes = Vec::new();
        let mut key_encoder = crate::enc::EncoderImpl::<_, C>::new(&mut key_bytes, *config);
        crate::enc::Encode::encode(key, &mut key_encoder).map_err(|_| {
            crate::error::cold_decode_error_other::<()>(
                "Failed to re-encode key for deterministic validation",
            )
            .unwrap_err()
        })?;
        self.validate_key_bytes(key_bytes)
    }
}

#[cfg(feature = "alloc")]
impl Default for DeterministicValidator {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
