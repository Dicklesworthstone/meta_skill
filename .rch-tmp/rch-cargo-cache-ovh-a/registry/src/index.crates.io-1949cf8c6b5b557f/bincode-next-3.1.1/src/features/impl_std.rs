use crate::config::Config;
use crate::config::internal::InternalFingerprintGuard;
use crate::de::BorrowDecode;
use crate::de::BorrowDecoder;
use crate::de::Decode;
use crate::de::Decoder;
use crate::de::DecoderImpl;
use crate::de::read::Reader;
use crate::enc::Encode;
use crate::enc::Encoder;
use crate::enc::EncoderImpl;
use crate::enc::write::Writer;
use crate::error::DecodeError;
use crate::error::EncodeError;
use crate::impl_borrow_decode;
use core::time::Duration;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::CStr;
use std::ffi::CString;
use std::hash::Hash;
use std::io::Read;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::net::SocketAddrV6;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::SystemTime;

/// Decode type `D` from the given reader with the given `Config`. The reader can be any type that implements `std::io::Read`, e.g. `std::fs::File`.
///
/// See the [config] module for more information about config options.
///
/// [config]: config/index.html
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
#[inline(always)]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub fn decode_from_std_read<D: Decode<()>, C: Config, R: std::io::Read>(
    src: &mut R,
    config: C,
) -> Result<D, DecodeError>
where
    C::Mode: crate::config::InternalFingerprintGuard<D, C>,
{
    decode_from_std_read_with_context(src, config, ())
}

/// Decode type `D` from the given reader with the given `Config` and `Context`. The reader can be any type that implements `std::io::Read`, e.g. `std::fs::File`.
///
/// See the [config] module for more information about config options.
///
/// [config]: config/index.html
///
/// # Errors
///
/// Returns a `DecodeError` if the reader fails or the data is invalid.
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[inline(always)]
pub fn decode_from_std_read_with_context<Context, D: Decode<Context>, C: Config, R: std::io::Read>(
    src: &mut R,
    config: C,
    context: Context,
) -> Result<D, DecodeError>
where
    C::Mode: crate::config::InternalFingerprintGuard<D, C>,
{
    let mut reader = IoReader::new(src);
    C::Mode::decode_check(&config, &mut reader)?;
    let mut decoder = DecoderImpl::<_, C, Context>::new(reader, config, context);
    D::decode(&mut decoder)
}

/// A reader that reads from a `std::io::Read`.
pub struct IoReader<R> {
    reader: R,
}

impl<R> IoReader<R> {
    /// Create a new `IoReader` from the given reader.
    pub const fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader for IoReader<R>
where
    R: std::io::Read,
{
    #[inline(always)]
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError> {
        self.reader.read_exact(bytes).map_err(|inner| {
            if inner.kind() == std::io::ErrorKind::UnexpectedEof {
                crate::error::cold_decode_error_unexpected_end::<()>(bytes.len()).unwrap_err()
            } else {
                crate::error::cold_decode_error_io::<()>(inner, bytes.len()).unwrap_err()
            }
        })
    }
}

impl<R> Reader for std::io::BufReader<R>
where
    R: std::io::Read,
{
    #[inline(always)]
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), DecodeError> {
        self.read_exact(bytes).map_err(|inner| {
            if inner.kind() == std::io::ErrorKind::UnexpectedEof {
                crate::error::cold_decode_error_unexpected_end::<()>(bytes.len()).unwrap_err()
            } else {
                crate::error::cold_decode_error_io::<()>(inner, bytes.len()).unwrap_err()
            }
        })
    }

    #[inline(always)]
    fn peek_read(
        &mut self,
        n: usize,
    ) -> Option<&[u8]> {
        self.buffer().get(..n)
    }

    #[inline(always)]
    fn consume(
        &mut self,
        n: usize,
    ) {
        <Self as std::io::BufRead>::consume(self, n);
    }
}

/// Encode the given value into any type that implements `std::io::Write`, e.g. `std::fs::File`, with the given `Config`.
///
/// See the [config] module for more information.
/// Returns the amount of bytes written.
///
/// [config]: config/index.html
///
/// # Errors
///
/// Returns an `EncodeError` if the writer fails.
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[inline]
pub fn encode_into_std_write<E: Encode, C: Config, W: std::io::Write>(
    val: E,
    dst: &mut W,
    config: C,
) -> Result<usize, EncodeError>
where
    C::Mode: crate::config::InternalFingerprintGuard<E, C>,
{
    let mut writer = IoWriter::new(dst);
    C::Mode::encode_check(&config, &mut writer)?;
    let mut encoder = EncoderImpl::<_, C>::new(writer, config);
    val.encode(&mut encoder)?;
    let mut final_writer = encoder.into_writer();
    final_writer.flush()?;
    Ok(final_writer.bytes_written())
}

/// A writer that writes to a `std::io::Write`.
pub struct IoWriter<'a, W: std::io::Write> {
    writer: &'a mut W,
    buffer: [u8; 2048],
    buffer_len: usize,
    bytes_written: usize,
}

impl<'a, W: std::io::Write> IoWriter<'a, W> {
    /// Create a new `IoWriter` from the given writer.
    pub const fn new(writer: &'a mut W) -> Self {
        Self {
            writer,
            buffer: [0; 2048],
            buffer_len: 0,
            bytes_written: 0,
        }
    }

    /// Returns the number of bytes written to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns `EncodeError` if the encoding fails.
    #[must_use]
    pub const fn bytes_written(&self) -> usize {
        self.bytes_written
    }

    /// Flushes any buffered data to the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns an `EncodeError` if the writer flushing fails.
    #[inline(always)]
    pub fn flush(&mut self) -> Result<(), EncodeError> {
        if self.buffer_len > 0 {
            self.writer
                .write_all(&self.buffer[..self.buffer_len])
                .map_err(|inner| {
                    crate::error::cold_encode_error_io::<()>(
                        inner,
                        self.bytes_written - self.buffer_len,
                    )
                    .unwrap_err()
                })?;
            self.buffer_len = 0;
        }
        Ok(())
    }
}

impl<W: std::io::Write> Writer for IoWriter<'_, W> {
    #[inline(always)]
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), EncodeError> {
        if crate::utils::is_likely!(bytes.len() <= self.buffer.len() - self.buffer_len) {
            self.buffer[self.buffer_len..self.buffer_len + bytes.len()].copy_from_slice(bytes);
            self.buffer_len += bytes.len();
        } else {
            self.flush()?;
            if crate::utils::is_likely!(bytes.len() >= self.buffer.len()) {
                self.writer.write_all(bytes).map_err(|inner| {
                    crate::error::cold_encode_error_io::<()>(inner, self.bytes_written).unwrap_err()
                })?;
            } else {
                self.buffer[..bytes.len()].copy_from_slice(bytes);
                self.buffer_len = bytes.len();
            }
        }
        self.bytes_written += bytes.len();
        Ok(())
    }
}

impl<W: std::io::Write> Drop for IoWriter<'_, W> {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl Encode for &CStr {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.to_bytes().encode(encoder)
    }
}

impl Encode for CString {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.as_bytes().encode(encoder)
    }
}

impl<Context> Decode<Context> for CString {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let vec = std::vec::Vec::decode(decoder)?;
        Self::new(vec).map_err(|inner| {
            crate::error::cold_decode_error_c_string_nul_error::<()>(inner.nul_position())
                .unwrap_err()
        })
    }
}
impl_borrow_decode!(CString);

impl<T> Encode for Mutex<T>
where
    T: Encode,
{
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        let t = self.lock().map_err(|_| {
            crate::error::cold_encode_error_lock_failed::<()>(core::any::type_name::<Self>())
                .unwrap_err()
        })?;
        t.encode(encoder)
    }
}

impl<Context, T> Decode<Context> for Mutex<T>
where
    T: Decode<Context>,
{
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let t = T::decode(decoder)?;
        Ok(Self::new(t))
    }
}
impl<'de, T, Context> BorrowDecode<'de, Context> for Mutex<T>
where
    T: BorrowDecode<'de, Context>,
{
    #[inline(always)]
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let t = T::borrow_decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl<T> Encode for RwLock<T>
where
    T: Encode,
{
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        let t = self.read().map_err(|_| {
            crate::error::cold_encode_error_lock_failed::<()>(core::any::type_name::<Self>())
                .unwrap_err()
        })?;
        t.encode(encoder)
    }
}

impl<Context, T> Decode<Context> for RwLock<T>
where
    T: Decode<Context>,
{
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let t = T::decode(decoder)?;
        Ok(Self::new(t))
    }
}
impl<'de, T, Context> BorrowDecode<'de, Context> for RwLock<T>
where
    T: BorrowDecode<'de, Context>,
{
    #[inline(always)]
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let t = T::borrow_decode(decoder)?;
        Ok(Self::new(t))
    }
}

impl Encode for SystemTime {
    #[inline]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        let duration = self.duration_since(Self::UNIX_EPOCH).map_err(|e| {
            crate::error::cold_encode_error_invalid_system_time::<()>(
                e,
                std::boxed::Box::new(*self),
            )
            .unwrap_err()
        })?;
        duration.encode(encoder)
    }
}

impl<Context> Decode<Context> for SystemTime {
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let duration = Duration::decode(decoder)?;
        Self::UNIX_EPOCH.checked_add(duration).ok_or_else(|| {
            crate::error::cold_decode_error_invalid_system_time::<()>(duration).unwrap_err()
        })
    }
}
impl_borrow_decode!(SystemTime);

impl Encode for &'_ Path {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.to_str()
            .ok_or_else(|| {
                crate::error::cold_encode_error_invalid_path_characters::<()>().unwrap_err()
            })?
            .encode(encoder)
    }
}

impl<'de, Context> BorrowDecode<'de, Context> for &'de Path {
    #[inline(always)]
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let str = <&'de str>::borrow_decode(decoder)?;
        Ok(Path::new(str))
    }
}

impl Encode for PathBuf {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.as_path().encode(encoder)
    }
}

impl<Context> Decode<Context> for PathBuf {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let string = std::string::String::decode(decoder)?;
        Ok(string.into())
    }
}
impl_borrow_decode!(PathBuf);

impl Encode for IpAddr {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        match self {
            | Self::V4(v4) => {
                0u32.encode(encoder)?;
                v4.encode(encoder)
            },
            | Self::V6(v6) => {
                1u32.encode(encoder)?;
                v6.encode(encoder)
            },
        }
    }
}

impl<Context> Decode<Context> for IpAddr {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match u32::decode(decoder)? {
            | 0 => Ok(Self::V4(Ipv4Addr::decode(decoder)?)),
            | 1 => Ok(Self::V6(Ipv6Addr::decode(decoder)?)),
            | found => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { min: 0, max: 1 },
                    found,
                )
            },
        }
    }
}
impl_borrow_decode!(IpAddr);

impl Encode for Ipv4Addr {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        encoder.writer().write(&self.octets())
    }
}

impl<Context> Decode<Context> for Ipv4Addr {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut buff = [0u8; 4];
        decoder.reader().read(&mut buff)?;
        Ok(Self::from(buff))
    }
}
impl_borrow_decode!(Ipv4Addr);

impl Encode for Ipv6Addr {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        encoder.writer().write(&self.octets())
    }
}

impl<Context> Decode<Context> for Ipv6Addr {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut buff = [0u8; 16];
        decoder.reader().read(&mut buff)?;
        Ok(Self::from(buff))
    }
}
impl_borrow_decode!(Ipv6Addr);

impl Encode for SocketAddr {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        match self {
            | Self::V4(v4) => {
                0u32.encode(encoder)?;
                v4.encode(encoder)
            },
            | Self::V6(v6) => {
                1u32.encode(encoder)?;
                v6.encode(encoder)
            },
        }
    }
}

impl<Context> Decode<Context> for SocketAddr {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        match u32::decode(decoder)? {
            | 0 => Ok(Self::V4(SocketAddrV4::decode(decoder)?)),
            | 1 => Ok(Self::V6(SocketAddrV6::decode(decoder)?)),
            | found => {
                crate::error::cold_decode_error_unexpected_variant(
                    core::any::type_name::<Self>(),
                    &crate::error::AllowedEnumVariants::Range { min: 0, max: 1 },
                    found,
                )
            },
        }
    }
}
impl_borrow_decode!(SocketAddr);

impl Encode for SocketAddrV4 {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.ip().encode(encoder)?;
        self.port().encode(encoder)
    }
}

impl<Context> Decode<Context> for SocketAddrV4 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let ip = Ipv4Addr::decode(decoder)?;
        let port = u16::decode(decoder)?;
        Ok(Self::new(ip, port))
    }
}
impl_borrow_decode!(SocketAddrV4);

impl Encode for SocketAddrV6 {
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        self.ip().encode(encoder)?;
        self.port().encode(encoder)?;
        if !encoder.config().is_compact_net() {
            self.flowinfo().encode(encoder)?;
            self.scope_id().encode(encoder)?;
        }
        Ok(())
    }
}

impl<Context> Decode<Context> for SocketAddrV6 {
    #[inline(always)]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let ip = Ipv6Addr::decode(decoder)?;
        let port = u16::decode(decoder)?;
        if decoder.config().is_compact_net() {
            Ok(Self::new(ip, port, 0, 0))
        } else {
            let flowinfo = u32::decode(decoder)?;
            let scope_id = u32::decode(decoder)?;
            Ok(Self::new(ip, port, flowinfo, scope_id))
        }
    }
}
impl_borrow_decode!(SocketAddrV6);

impl<K, V, S> Encode for HashMap<K, V, S>
where
    K: Encode,
    V: Encode,
{
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        let format = <E::C as crate::config::InternalFormatConfig>::FORMAT;
        if format == Format::CborDeterministic {
            crate::enc::cbor::encode_map_deterministic::<E, _, _, _>(encoder, self.iter())
        } else if format == Format::BincodeDeterministic {
            #[cfg(feature = "alloc")]
            return crate::enc::deterministic::encode_map_deterministic::<E, _, _, _>(
                encoder,
                self.iter(),
            );
            #[cfg(not(feature = "alloc"))]
            return crate::error::cold_encode_error_other(
                "Deterministic encoding requires the 'alloc' feature",
            );
        } else {
            encoder.encode_map_len(self.len())?;
            for (k, v) in self {
                Encode::encode(k, encoder)?;
                Encode::encode(v, encoder)?;
            }
            Ok(())
        }
    }
}

impl<Context, K, V, S> Decode<Context> for HashMap<K, V, S>
where
    K: Decode<Context> + Eq + std::hash::Hash,
    V: Decode<Context>,
    S: std::hash::BuildHasher + Default,
{
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = decoder.decode_map_len()?;
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );
        let is_deterministic = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::BincodeDeterministic
        );
        if !is_bincode && len == usize::MAX {
            let mut map = Self::with_hasher(S::default());
            while decoder.reader().peek_u8() != Some(0xFF) {
                let k = K::decode(decoder)?;
                let v = V::decode(decoder)?;
                map.insert(k, v);
            }
            decoder.reader().read_u8()?; // consume 0xFF
            return Ok(map);
        }
        decoder.claim_container_read::<(K, V)>(len)?;

        let hash_builder: S = Default::default();
        let mut map = Self::with_capacity_and_hasher(len, hash_builder);
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<(K, V)>());
            let k = K::decode(decoder)?;
            let v = V::decode(decoder)?;
            if is_deterministic {
                if map.insert(k, v).is_some() {
                    return crate::error::cold_decode_error_duplicate_map_key();
                }
            } else {
                map.insert(k, v);
            }
        }
        Ok(map)
    }
}
impl<'de, K, V, S, Context> BorrowDecode<'de, Context> for HashMap<K, V, S>
where
    K: BorrowDecode<'de, Context> + Eq + std::hash::Hash,
    V: BorrowDecode<'de, Context>,
    S: std::hash::BuildHasher + Default,
{
    #[inline]
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let len = decoder.decode_map_len()?;
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );
        let is_deterministic = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::BincodeDeterministic
        );
        if !is_bincode && len == usize::MAX {
            let mut map = Self::with_hasher(S::default());
            while decoder.reader().peek_u8() != Some(0xFF) {
                let k = K::borrow_decode(decoder)?;
                let v = V::borrow_decode(decoder)?;
                map.insert(k, v);
            }
            decoder.reader().read_u8()?; // consume 0xFF
            return Ok(map);
        }
        decoder.claim_container_read::<(K, V)>(len)?;

        let hash_builder: S = Default::default();
        let mut map = Self::with_capacity_and_hasher(len, hash_builder);
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<(K, V)>());
            let k = K::borrow_decode(decoder)?;
            let v = V::borrow_decode(decoder)?;
            if is_deterministic {
                if map.insert(k, v).is_some() {
                    return crate::error::cold_decode_error_duplicate_map_key();
                }
            } else {
                map.insert(k, v);
            }
        }
        Ok(map)
    }
}

impl<Context, T, S> Decode<Context> for HashSet<T, S>
where
    T: Decode<Context> + Eq + Hash,
    S: std::hash::BuildHasher + Default,
{
    #[inline]
    fn decode<D: Decoder<Context = Context>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let len = decoder.decode_slice_len()?;
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );
        let is_deterministic = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::BincodeDeterministic
        );
        if !is_bincode && len == usize::MAX {
            let mut map = Self::with_hasher(S::default());
            while decoder.reader().peek_u8() != Some(0xFF) {
                let key = T::decode(decoder)?;
                map.insert(key);
            }
            decoder.reader().read_u8()?; // consume 0xFF
            return Ok(map);
        }
        decoder.claim_container_read::<T>(len)?;

        let hash_builder: S = Default::default();
        let mut map: Self = Self::with_capacity_and_hasher(len, hash_builder);
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<T>());
            let key = T::decode(decoder)?;
            if is_deterministic {
                if !map.insert(key) {
                    return crate::error::cold_decode_error_duplicate_map_key();
                }
            } else {
                map.insert(key);
            }
        }
        Ok(map)
    }
}

impl<'de, T, S, Context> BorrowDecode<'de, Context> for HashSet<T, S>
where
    T: BorrowDecode<'de, Context> + Eq + Hash,
    S: std::hash::BuildHasher + Default,
{
    #[inline]
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D
    ) -> Result<Self, DecodeError> {
        let len = decoder.decode_slice_len()?;
        let is_bincode = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::Bincode | crate::config::Format::BincodeDeterministic
        );
        let is_deterministic = matches!(
            <D::C as crate::config::InternalFormatConfig>::FORMAT,
            crate::config::Format::BincodeDeterministic
        );
        if !is_bincode && len == usize::MAX {
            let mut map = Self::with_hasher(S::default());
            while decoder.reader().peek_u8() != Some(0xFF) {
                let key = T::borrow_decode(decoder)?;
                map.insert(key);
            }
            decoder.reader().read_u8()?; // consume 0xFF
            return Ok(map);
        }
        decoder.claim_container_read::<T>(len)?;

        let mut map = Self::with_capacity_and_hasher(len, S::default());
        for _ in 0..len {
            decoder.unclaim_bytes_read(core::mem::size_of::<T>());
            let key = T::borrow_decode(decoder)?;
            if is_deterministic {
                if !map.insert(key) {
                    return crate::error::cold_decode_error_duplicate_map_key();
                }
            } else {
                map.insert(key);
            }
        }
        Ok(map)
    }
}

impl<T, S> Encode for HashSet<T, S>
where
    T: Encode,
{
    #[inline(always)]
    fn encode<E: Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), EncodeError> {
        use crate::config::Format;
        let format = <E::C as crate::config::InternalFormatConfig>::FORMAT;
        if format == Format::CborDeterministic {
            crate::enc::cbor::encode_slice_deterministic::<E, _, _>(encoder, self.iter())
        } else if format == Format::BincodeDeterministic {
            #[cfg(feature = "alloc")]
            return crate::enc::deterministic::encode_slice_deterministic::<E, _, _>(
                encoder,
                self.iter(),
            );
            #[cfg(not(feature = "alloc"))]
            return crate::error::cold_encode_error_other(
                "Deterministic encoding requires the 'alloc' feature",
            );
        } else {
            encoder.encode_slice_len(self.len())?;
            for item in self {
                item.encode(encoder)?;
            }
            Ok(())
        }
    }
}
