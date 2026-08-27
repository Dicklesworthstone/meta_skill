use crate::enc::write::Writer;
use crate::error::EncodeError;

/// A wrapper around a `Writer` that allows writing data bit-by-bit.
pub struct BitWriter<'a, W: Writer> {
    writer: &'a mut W,
    current_byte: u8,
    bit_count: u8,
}

impl<'a, W: Writer> BitWriter<'a, W> {
    /// Creates a new `BitWriter`.
    #[inline(always)]
    pub const fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            current_byte: 0,
            bit_count: 0,
        }
    }

    /// Creates a new `BitWriter` with an existing state.
    #[inline(always)]
    pub const fn from_state(
        writer: &'a mut W,
        current_byte: u8,
        bit_count: u8,
    ) -> Self {
        Self {
            writer,
            current_byte,
            bit_count,
        }
    }

    /// Returns the current state of the bit writer (`current_byte`, `bit_count`).
    #[inline(always)]
    #[must_use]
    pub const fn get_state(&self) -> (u8, u8) {
        (self.current_byte, self.bit_count)
    }

    /// Writes `num_bits` from `val` into the stream, using LSB-first bit ordering.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the underlying writer fails to write the data.
    #[inline(always)]
    pub fn write_bits_lsb(
        &mut self,
        mut val: u64,
        mut num_bits: u8,
    ) -> Result<(), EncodeError> {
        while num_bits > 0 {
            let space_in_byte = 8 - self.bit_count;
            let bits_to_write = num_bits.min(space_in_byte);

            let mask = (1u64 << bits_to_write) - 1;
            let chunk = (val & mask) as u8;

            self.current_byte |= chunk << self.bit_count;
            self.bit_count += bits_to_write;

            val >>= bits_to_write;
            num_bits -= bits_to_write;

            if self.bit_count == 8 {
                self.flush()?;
            }
        }
        Ok(())
    }

    /// Writes `num_bits` from `val` into the stream, using MSB-first bit ordering.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the underlying writer fails to write the data.
    #[inline(always)]
    pub fn write_bits_msb(
        &mut self,
        val: u64,
        mut num_bits: u8,
    ) -> Result<(), EncodeError> {
        while num_bits > 0 {
            let space_in_byte = 8 - self.bit_count;
            let bits_to_write = num_bits.min(space_in_byte);

            // Extract the top `bits_to_write` from the `num_bits` we care about in `val`
            let shift_down = num_bits - bits_to_write;
            let mask = (1u64 << bits_to_write) - 1;
            let chunk = ((val >> shift_down) & mask) as u8;

            let shift_up = 8 - self.bit_count - bits_to_write;
            self.current_byte |= chunk << shift_up;
            self.bit_count += bits_to_write;

            num_bits -= bits_to_write;

            if self.bit_count == 8 {
                self.flush()?;
            }
        }
        Ok(())
    }

    /// Writes `num_bits` from `val` into the stream, using the bit ordering from the configuration.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the underlying writer fails to write the data.
    #[inline(always)]
    pub fn write_bits<C: crate::config::Config>(
        &mut self,
        val: u64,
        num_bits: u8,
        config: &C,
    ) -> Result<(), EncodeError> {
        use crate::config::BitOrdering;
        match config.bit_ordering() {
            | BitOrdering::Lsb => self.write_bits_lsb(val, num_bits),
            | BitOrdering::Msb => self.write_bits_msb(val, num_bits),
        }
    }

    /// Flushes any remaining bits to the underlying writer, padding with 0s.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the underlying writer fails to write the data.
    #[inline(always)]
    pub fn flush(&mut self) -> Result<(), EncodeError> {
        if self.bit_count > 0 {
            self.writer.write(&[self.current_byte])?;
            self.current_byte = 0;
            self.bit_count = 0;
        }
        Ok(())
    }
}
