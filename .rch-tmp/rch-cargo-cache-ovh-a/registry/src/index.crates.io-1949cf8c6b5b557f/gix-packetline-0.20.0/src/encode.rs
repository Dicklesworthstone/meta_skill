use super::MAX_DATA_LEN;

/// The error returned by most functions in the [`encode`](crate::encode) module
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Cannot encode more than {MAX_DATA_LEN} bytes, got {length_in_bytes}")]
    DataLengthLimitExceeded { length_in_bytes: usize },
    #[error("Empty lines are invalid")]
    DataIsEmpty,
}

pub(crate) fn u16_to_hex(value: u16) -> [u8; 4] {
    let mut buf = [0u8; 4];
    faster_hex::hex_encode(&value.to_be_bytes(), &mut buf).expect("two bytes to 4 hex chars never fails");
    buf
}
