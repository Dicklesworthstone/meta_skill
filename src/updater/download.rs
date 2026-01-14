//! Binary download

use crate::error::{MsError, Result};

/// Download update binary
pub fn download(_url: &str) -> Result<Vec<u8>> {
    Err(MsError::NotImplemented(
        "binary download is not implemented yet".to_string(),
    ))
}
