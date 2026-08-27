use super::Encoder;
use super::write::Writer;
use crate::config::Config;
use crate::error_path::BincodeErrorPathCovered;
use crate::utils::Sealed;

impl<W: Writer, C: Config> BincodeErrorPathCovered<1> for EncoderImpl<W, C> {}

/// An Encoder that writes bytes into a given writer `W`.
///
/// This struct should rarely be used.
/// In most cases, prefer any of the `encode` functions.
///
/// The `ByteOrder` that is chosen will impact the endianness that
/// is used to write integers to the writer.
///
/// ```
/// # use bincode_next::enc::{write::SliceWriter, EncoderImpl, Encode};
/// let slice: &mut [u8] = &mut [0, 0, 0, 0];
/// let config = bincode_next::config::legacy().with_big_endian();
///
/// let mut encoder = EncoderImpl::new(SliceWriter::new(slice), config);
/// // this u32 can be any Encodable
/// 5u32.encode(&mut encoder).unwrap();
/// assert_eq!(encoder.into_writer().bytes_written(), 4);
/// assert_eq!(slice, [0, 0, 0, 5]);
/// ```
pub struct EncoderImpl<W: Writer, C: Config> {
    writer: W,
    config: C,
}

impl<W: Writer, C: Config> EncoderImpl<W, C> {
    /// Create a new Encoder
    pub const fn new(
        writer: W,
        config: C,
    ) -> Self {
        Self { writer, config }
    }

    /// Return the underlying writer
    #[inline(always)]
    pub fn into_writer(self) -> W {
        self.writer
    }
}

impl<W: Writer, C: Config> Encoder for EncoderImpl<W, C> {
    type C = C;
    type W = W;

    #[inline(always)]
    fn writer(&mut self) -> &mut Self::W {
        &mut self.writer
    }

    #[inline(always)]
    fn config(&self) -> &Self::C {
        &self.config
    }
}

impl<W: Writer, C: Config> Sealed for EncoderImpl<W, C> {}
