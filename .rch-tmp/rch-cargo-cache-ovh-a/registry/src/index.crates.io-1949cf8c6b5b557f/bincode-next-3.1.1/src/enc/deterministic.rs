#[cfg(feature = "alloc")]
use crate::alloc::vec::Vec;
use crate::enc::Encode;
use crate::enc::Encoder;
use crate::enc::write::Writer;
use crate::error::EncodeError;

/// Encode a map with deterministic ordering.
///
/// Keys are sorted by the bytewise lexicographic order of their bincode encodings.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[cfg(feature = "alloc")]
#[inline(always)]
pub fn encode_map_deterministic<E, K, V, I>(
    encoder: &mut E,
    iter: I,
) -> Result<(), EncodeError>
where
    E: Encoder,
    K: Encode,
    V: Encode,
    I: IntoIterator<Item = (K, V)>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = iter.into_iter();
    let len = iter.len();
    encoder.encode_map_len(len)?;

    let mut entries = Vec::with_capacity(len);
    for (k, v) in iter {
        let mut key_bytes = Vec::new();
        let mut key_encoder =
            crate::enc::EncoderImpl::<_, E::C>::new(&mut key_bytes, *encoder.config());
        k.encode(&mut key_encoder)?;
        entries.push((key_bytes, v));
    }

    entries.sort_by(|(ka, _), (kb, _)| ka.cmp(kb));

    for (k_bytes, v) in entries {
        encoder.writer().write(&k_bytes)?;
        v.encode(encoder)?;
    }

    Ok(())
}

/// Encode a slice with deterministic ordering.
///
/// Elements are sorted by the bytewise lexicographic order of their bincode encodings.
///
/// # Errors
///
/// Returns `EncodeError` if the encoding fails.
#[cfg(feature = "alloc")]
#[inline]
pub fn encode_slice_deterministic<E, T, I>(
    encoder: &mut E,
    iter: I,
) -> Result<(), EncodeError>
where
    E: Encoder,
    T: Encode,
    I: IntoIterator<Item = T>,
    I::IntoIter: ExactSizeIterator,
{
    let iter = iter.into_iter();
    let len = iter.len();
    encoder.encode_slice_len(len)?;

    let mut entries = Vec::with_capacity(len);
    for item in iter {
        let mut bytes = Vec::new();
        let mut key_encoder =
            crate::enc::EncoderImpl::<_, E::C>::new(&mut bytes, *encoder.config());
        item.encode(&mut key_encoder)?;
        entries.push(bytes);
    }

    entries.sort();

    for bytes in entries {
        encoder.writer().write(&bytes)?;
    }

    Ok(())
}
