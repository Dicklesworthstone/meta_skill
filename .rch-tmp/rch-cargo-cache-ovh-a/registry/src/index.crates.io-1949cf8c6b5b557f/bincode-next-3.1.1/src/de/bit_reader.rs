use crate::de::read::Reader;
use crate::error::DecodeError;

/// A wrapper around a `Reader` that allows reading data bit-by-bit.
pub struct BitReader<'a, R: Reader> {
    reader: &'a mut R,
    current_byte: u8,
    bits_available: u8,
}

impl<'a, R: Reader> BitReader<'a, R> {
    /// Creates a new `BitReader`.
    #[inline(always)]
    pub const fn new(reader: &'a mut R) -> Self {
        Self {
            reader,
            current_byte: 0,
            bits_available: 0,
        }
    }

    /// Creates a new `BitReader` with an existing state.
    #[inline(always)]
    pub const fn from_state(
        reader: &'a mut R,
        current_byte: u8,
        bits_available: u8,
    ) -> Self {
        Self {
            reader,
            current_byte,
            bits_available,
        }
    }

    /// Returns the current state of the bit reader (`current_byte`, `bits_available`).
    #[inline(always)]
    #[must_use]
    pub const fn get_state(&self) -> (u8, u8) {
        (self.current_byte, self.bits_available)
    }

    /// Reads `num_bits` from the stream, using LSB-first bit ordering.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the underlying reader fails to provide enough bytes.
    #[inline]
    pub fn read_bits_lsb(
        &mut self,
        mut num_bits: u8,
    ) -> Result<u64, DecodeError> {
        let mut result: u64 = 0;
        let mut bits_read: u8 = 0;

        while num_bits > 0 {
            if self.bits_available == 0 {
                let mut buf = [0u8; 1];
                self.reader.read(&mut buf)?;
                self.current_byte = buf[0];
                self.bits_available = 8;
            }

            let bits_to_read = num_bits.min(self.bits_available);
            let mask = ((1u16 << bits_to_read) - 1) as u8;

            let chunk = self.current_byte & mask;
            result |= u64::from(chunk) << bits_read;

            self.current_byte >>= bits_to_read;
            self.bits_available -= bits_to_read;

            bits_read += bits_to_read;
            num_bits -= bits_to_read;
        }

        Ok(result)
    }

    /// Reads `num_bits` from the stream, using MSB-first bit ordering.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the underlying reader fails to provide enough bytes.
    #[inline]
    pub fn read_bits_msb(
        &mut self,
        mut num_bits: u8,
    ) -> Result<u64, DecodeError> {
        let mut result: u64 = 0;

        while num_bits > 0 {
            if self.bits_available == 0 {
                let mut buf = [0u8; 1];
                self.reader.read(&mut buf)?;
                self.current_byte = buf[0];
                self.bits_available = 8;
            }

            let bits_to_read = num_bits.min(self.bits_available);
            let shift_down = self.bits_available - bits_to_read;
            let mask = ((1u16 << bits_to_read) - 1) as u8;

            let chunk = (self.current_byte >> shift_down) & mask;
            result = (result << bits_to_read) | u64::from(chunk);

            self.bits_available -= bits_to_read;
            num_bits -= bits_to_read;
        }

        Ok(result)
    }

    /// Reads `num_bits` from the stream, using the bit ordering from the configuration.
    ///
    /// # Errors
    ///
    /// Returns `DecodeError` if the underlying reader fails to provide enough bytes.
    #[inline(always)]
    pub fn read_bits<C: crate::config::Config>(
        &mut self,
        num_bits: u8,
        config: &C,
    ) -> Result<u64, DecodeError> {
        use crate::config::BitOrdering;
        match config.bit_ordering() {
            | BitOrdering::Lsb => self.read_bits_lsb(num_bits),
            | BitOrdering::Msb => self.read_bits_msb(num_bits),
        }
    }

    /// Discards any remaining unread bits in the current byte, effectively returning to byte alignment.
    #[inline(always)]
    pub const fn align_to_byte(&mut self) {
        self.bits_available = 0;
        self.current_byte = 0;
    }
}

/// A helper trait to unpack `u64` into various types for the `BitPacked` macro.
pub trait Unpackable {
    /// Convert the unpacked `u64` to `Self`.
    fn unpack(val: u64) -> Self;
}

impl Unpackable for bool {
    #[inline(always)]
    fn unpack(val: u64) -> Self {
        val != 0
    }
}

macro_rules! impl_unpackable_int {
    ($($ty:ty),*) => {
        $(
            impl Unpackable for $ty {
                #[inline(always)]
                fn unpack(val: u64) -> Self {
                    val as $ty
                }
            }
        )*
    };
}

impl_unpackable_int!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);
