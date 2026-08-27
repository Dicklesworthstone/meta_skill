use super::BorrowDecoder;
use super::Decoder;
use super::read::BorrowReader;
use super::read::Reader;
use crate::config::Config;
use crate::error::DecodeError;
use crate::error_path::BincodeErrorPathCovered;
use crate::utils::Sealed;

impl<R, C: Config, Context> BincodeErrorPathCovered<0> for DecoderImpl<R, C, Context> {}
impl<C, D: Decoder + ?Sized> BincodeErrorPathCovered<0> for WithContext<'_, D, C> {}

/// A Decoder that reads bytes from a given reader `R`.
///
/// This struct should rarely be used.
/// In most cases, prefer any of the `decode` functions.
///
/// The `ByteOrder` that is chosen will impact the endianness that
/// is used to read integers out of the reader.
///
/// ```
/// # let slice: &[u8] = &[0, 0, 0, 0];
/// # let some_reader = bincode_next::de::read::SliceReader::new(slice);
/// use bincode_next::de::Decode;
/// use bincode_next::de::DecoderImpl;
/// let mut context = ();
/// let mut decoder = DecoderImpl::new(some_reader, bincode_next::config::standard(), &mut context);
/// // this u32 can be any Decode
/// let value = u32::decode(&mut decoder).unwrap();
/// ```
pub struct DecoderImpl<R, C: Config, Context> {
    reader: R,
    config: C,
    bytes_read: usize,
    context: Context,
}

impl<R: Reader, C: Config, Context> DecoderImpl<R, C, Context> {
    /// Construct a new Decoder
    #[inline(always)]
    pub const fn new(
        reader: R,
        config: C,
        context: Context,
    ) -> Self {
        Self {
            reader,
            config,
            bytes_read: 0,
            context,
        }
    }
}

impl<R, C: Config, Context> Sealed for DecoderImpl<R, C, Context> {}

impl<'de, R: BorrowReader<'de>, C: Config, Context> BorrowDecoder<'de>
    for DecoderImpl<R, C, Context>
{
    type BR = R;

    #[inline(always)]
    fn borrow_reader(&mut self) -> &mut Self::BR {
        &mut self.reader
    }
}

impl<R: Reader, C: Config, Context> Decoder for DecoderImpl<R, C, Context> {
    type C = C;
    type Context = Context;
    type R = R;

    #[inline(always)]
    fn reader(&mut self) -> &mut Self::R {
        &mut self.reader
    }

    #[inline(always)]
    fn config(&self) -> &Self::C {
        &self.config
    }

    #[inline(always)]
    fn claim_bytes_read(
        &mut self,
        n: usize,
    ) -> Result<(), DecodeError> {
        Self::assert_covered();
        // C::LIMIT is a const so this check should get compiled away
        if let Some(limit) = C::LIMIT {
            // Make sure we don't accidentally overflow `bytes_read`
            if let Some(sum) = self.bytes_read.checked_add(n) {
                if sum > limit {
                    return crate::error::cold_decode_error_limit_exceeded();
                }
                self.bytes_read = sum;
                Ok(())
            } else {
                crate::error::cold_decode_error_limit_exceeded()
            }
        } else {
            Ok(())
        }
    }

    #[inline(always)]
    fn unclaim_bytes_read(
        &mut self,
        n: usize,
    ) {
        // C::LIMIT is a const so this check should get compiled away
        if C::LIMIT.is_some() {
            // We should always be claiming more than we unclaim, so this should never underflow
            self.bytes_read -= n;
        }
    }

    #[inline(always)]
    fn context(&mut self) -> &mut Self::Context {
        &mut self.context
    }
}

pub struct WithContext<'a, D: ?Sized, C> {
    pub(crate) decoder: &'a mut D,
    pub(crate) context: C,
}

impl<C, D: Decoder + ?Sized> Sealed for WithContext<'_, D, C> {}

impl<Context, D: Decoder + ?Sized> Decoder for WithContext<'_, D, Context> {
    type C = D::C;
    type Context = Context;
    type R = D::R;

    #[inline(always)]
    fn context(&mut self) -> &mut Self::Context {
        &mut self.context
    }

    #[inline(always)]
    fn reader(&mut self) -> &mut Self::R {
        self.decoder.reader()
    }

    #[inline(always)]
    fn config(&self) -> &Self::C {
        self.decoder.config()
    }

    #[inline(always)]
    fn claim_bytes_read(
        &mut self,
        n: usize,
    ) -> Result<(), DecodeError> {
        Self::assert_covered();
        self.decoder.claim_bytes_read(n)
    }

    #[inline(always)]
    fn unclaim_bytes_read(
        &mut self,
        n: usize,
    ) {
        self.decoder.unclaim_bytes_read(n);
    }
}

impl<'de, C, D: BorrowDecoder<'de>> BorrowDecoder<'de> for WithContext<'_, D, C> {
    type BR = D::BR;

    #[inline(always)]
    fn borrow_reader(&mut self) -> &mut Self::BR {
        self.decoder.borrow_reader()
    }
}
