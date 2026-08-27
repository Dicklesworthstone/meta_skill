//! The config module is used to change the behavior of bincode's encoding and decoding logic.
//!
//! *Important* make sure you use the same config for encoding and decoding, or else bincode will not work properly.
//!
//! To use a config, first create a type of [Configuration]. This type will implement trait [Config] for use with bincode.
//!
//! ```
//! let config = bincode_next::config::standard()
//!     // pick one of:
//!     .with_big_endian()
//!     .with_little_endian()
//!     // pick one of:
//!     .with_variable_int_encoding()
//!     .with_fixed_int_encoding()
//!     // pick one of:
//!     .with_bit_packing()
//!     .with_no_bit_packing();
//! ```
//!
//! See [Configuration] for more information on the configuration options.

#[doc(hidden)]
pub use self::internal::*;
use core::marker::PhantomData;

/// The Configuration struct is used to build bincode configurations. The [Config] trait is implemented
/// by this struct when a valid configuration has been constructed.
///
/// The following methods are mutually exclusive and will overwrite each other. The last call to one of these methods determines the behavior of the configuration:
///
/// - [`with_little_endian`\] and [`with_big_endian`\]
/// - [`with_fixed_int_encoding`\] and [`with_variable_int_encoding`\]
/// - [`with_bit_packing`\] and [`with_no_bit_packing`\]
///
///
/// [with_little_endian]: #method.with_little_endian
/// [with_big_endian]: #method.with_big_endian
/// [with_fixed_int_encoding]: #method.with_fixed_int_encoding
/// [with_variable_int_encoding]: #method.with_variable_int_encoding
/// [with_bit_packing]: #method.with_bit_packing
/// [with_no_bit_packing]: #method.with_no_bit_packing
#[derive(Copy, Clone, Debug)]
pub struct Configuration<
    E = LittleEndian,
    I = Varint,
    L = NoLimit,
    B = SkipBitPacking,
    O = LsbFirst,
    F = FingerprintDisabled,
    FO = BincodeFormat,
> {
    _e: PhantomData<E>,
    _i: PhantomData<I>,
    _l: PhantomData<L>,
    _b: PhantomData<B>,
    _o: PhantomData<O>,
    _f: PhantomData<F>,
    _fo: PhantomData<FO>,
}

/// The modes for CBOR deterministic encoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CborDeterministicMode {
    /// No deterministic encoding requirements.
    None = 0,
    /// Core deterministic encoding (RFC 8949 §4.2.1).
    Core = 1,
    /// Length-first deterministic encoding (RFC 8949 §4.2.3).
    LengthFirst = 2,
}

/// Detailed options for CBOR encoding.
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CborOptions {
    /// Deterministic encoding mode.
    pub deterministic_mode: CborDeterministicMode,
    /// Preferred serialization for floating-point values (§4.1).
    pub preferred_float: bool,
    /// Canonical NaN encoding (0xf97e00).
    pub canonical_nan: bool,
    /// Normalize negative zero (-0.0) to positive zero (0.0).
    pub normalize_neg_zero: bool,
    /// Allow indefinite-length items.
    pub allow_indefinite: bool,
    /// Strict tag validity checking.
    pub strict_tags: bool,
}

impl CborOptions {
    /// Default CBOR options.
    pub const DEFAULT: Self = Self {
        deterministic_mode: CborDeterministicMode::None,
        preferred_float: true,
        canonical_nan: true,
        normalize_neg_zero: false,
        allow_indefinite: true,
        strict_tags: false,
    };
    /// Default options for deterministic CBOR.
    pub const DETERMINISTIC: Self = Self {
        deterministic_mode: CborDeterministicMode::Core,
        preferred_float: true,
        canonical_nan: true,
        normalize_neg_zero: false, // RFC 8949 §4.2.1: MUST NOT normalize -0.0
        allow_indefinite: false,
        strict_tags: true,
    };
}

// When adding more features to configuration, follow these steps:
// - Create 2 or more structs that can be used as a type (e.g. Limit and NoLimit)
// - Add an `Internal...Config` to the `internal` module
// - Make sure `Config` and `impl<T> Config for T` extend from this new trait
// - Add a generic to `Configuration`
// - Add this generic to `impl<...> Default for Configuration<...>`
// - Add this generic to `const fn generate<...>()`
// - Add this generic to _every_ function in `Configuration`
// - Add your new methods

/// The default config for bincode 2.0. By default this will be:
/// - Little endian
/// - Variable int encoding
#[must_use]
pub const fn standard() -> Configuration {
    generate()
}

/// Creates the "legacy" default config. This is the default config that was present in bincode 1.0
/// - Little endian
/// - Fixed int length encoding
#[must_use]
pub const fn legacy() -> Configuration<
    LittleEndian,
    Fixint,
    NoLimit,
    SkipBitPacking,
    LsbFirst,
    FingerprintDisabled,
    BincodeLegacyFormat,
> {
    generate()
}

impl<E, I, L, B, O, F, FO> Default for Configuration<E, I, L, B, O, F, FO> {
    fn default() -> Self {
        generate()
    }
}

const fn generate<E, I, L, B, O, F, FO>() -> Configuration<E, I, L, B, O, F, FO> {
    Configuration {
        _e: PhantomData,
        _i: PhantomData,
        _l: PhantomData,
        _b: PhantomData,
        _o: PhantomData,
        _f: PhantomData,
        _fo: PhantomData,
    }
}

impl<E, I, L, B, O, F, FO> Configuration<E, I, L, B, O, F, FO> {
    /// Makes bincode encode all integer types in big endian.
    #[must_use]
    pub const fn with_big_endian(self) -> Configuration<BigEndian, I, L, B, MsbFirst, F, FO> {
        generate()
    }

    /// Makes bincode encode all integer types in little endian.
    #[must_use]
    pub const fn with_little_endian(self) -> Configuration<LittleEndian, I, L, B, LsbFirst, F, FO> {
        generate()
    }

    /// Makes bincode encode all integer types with a variable integer encoding.
    ///
    /// Encoding an unsigned integer `v` (of any type excepting `u8`) works as follows:
    ///
    /// 1. If `u < 251`, encode it as a single byte with that value.
    /// 2. If `251 <= u < 2**16`, encode it as a literal byte `251`, followed by a `u16` with value `u`.
    /// 3. If `2**16 <= u < 2**32`, encode it as a literal byte `252`, followed by a `u32` with value `u`.
    /// 4. If `2**32 <= u < 2**64`, encode it as a literal byte `253`, followed by a `u64` with value `u`.
    /// 5. If `2**64 <= u < 2**128`, encode it as a literal byte `254`, followed by a `u128` with value `u`.
    ///
    /// Then, for signed integers, we first convert to unsigned using the zigzag algorithm,
    /// and then encode them as we do for unsigned integers generally. The reason we use this
    /// algorithm is that it encodes those values which are close to zero in less bytes; the
    /// obvious algorithm, where we encode the cast values, gives a very large encoding for all
    /// negative values.
    ///
    /// The zigzag algorithm is defined as follows:
    ///
    /// ```rust
    /// # type Signed = i32;
    /// # type Unsigned = u32;
    /// fn zigzag(v: Signed) -> Unsigned {
    ///     match v {
    ///         | 0 => 0,
    ///         // To avoid the edge case of Signed::min_value()
    ///         // !n is equal to `-n - 1`, so this is:
    ///         // !n * 2 + 1 = 2(-n - 1) + 1 = -2n - 2 + 1 = -2n - 1
    ///         | v if v < 0 => !(v as Unsigned) * 2 - 1,
    ///         | v if v > 0 => (v as Unsigned) * 2,
    /// #       _ => unreachable!()
    ///     }
    /// }
    /// ```
    ///
    /// And works such that:
    ///
    /// ```rust
    /// # let zigzag = |n: i64| -> u64 {
    /// #     match n {
    /// #         0 => 0,
    /// #         v if v < 0 => !(v as u64) * 2 + 1,
    /// #         v if v > 0 => (v as u64) * 2,
    /// #         _ => unreachable!(),
    /// #     }
    /// # };
    /// assert_eq!(zigzag(0), 0);
    /// assert_eq!(zigzag(-1), 1);
    /// assert_eq!(zigzag(1), 2);
    /// assert_eq!(zigzag(-2), 3);
    /// assert_eq!(zigzag(2), 4);
    /// // etc
    /// assert_eq!(zigzag(i64::min_value()), u64::max_value());
    /// ```
    ///
    /// Note that u256 and the like are unsupported by this format; if and when they are added to the
    /// language, they may be supported via the extension point given by the 255 byte.
    #[must_use]
    pub const fn with_variable_int_encoding(self) -> Configuration<E, Varint, L, B, O, F, FO> {
        generate()
    }

    /// Fixed-size integer encoding.
    ///
    /// * Fixed size integers are encoded directly
    /// * Enum discriminants are encoded as u32
    /// * Lengths and usize are encoded as u64
    #[must_use]
    pub const fn with_fixed_int_encoding(self) -> Configuration<E, Fixint, L, B, O, F, FO> {
        generate()
    }

    /// Sets the byte limit to `limit`.
    #[must_use]
    pub const fn with_limit<const N: usize>(self) -> Configuration<E, I, Limit<N>, B, O, F, FO> {
        generate()
    }

    /// Clear the byte limit.
    #[must_use]
    pub const fn with_no_limit(self) -> Configuration<E, I, NoLimit, B, O, F, FO> {
        generate()
    }

    /// Enables bit-packing for types that support it.
    #[must_use]
    pub const fn with_bit_packing(self) -> Configuration<E, I, L, AllowBitPacking, O, F, FO> {
        generate()
    }

    /// Disables bit-packing.
    #[must_use]
    pub const fn with_no_bit_packing(self) -> Configuration<E, I, L, SkipBitPacking, O, F, FO> {
        generate()
    }

    /// Enables fingerprinting with the default deterministic seed (0).
    ///
    /// The fingerprint is a 64-bit hash that covers:
    /// - The type name
    /// - For structs: field names, types, and their order
    /// - For enums: variant names, field names, types, and their order
    /// - Configuration settings such as endianness and integer encoding
    #[must_use]
    pub const fn with_fingerprint(self) -> Configuration<E, I, L, B, O, FingerprintEnabled<0>, FO> {
        generate()
    }

    /// Enables fingerprinting with a custom seed.
    ///
    /// This is similar to [`with_fingerprint`](Self::with_fingerprint), but allows specifying a custom seed
    /// to further differentiate fingerprints.
    #[must_use]
    pub const fn with_fingerprint_and_seed<const SEED: u64>(
        self
    ) -> Configuration<E, I, L, B, O, FingerprintEnabled<SEED>, FO> {
        generate()
    }

    /// Enables legacy fingerprinting with an expected hash.
    ///
    /// In this mode, bincode will not calculate the fingerprint automatically.
    /// Instead, it will use the provided `EXPECTED` hash for verification.
    #[must_use]
    pub const fn with_legacy_fingerprint<const EXPECTED: u64>(
        self
    ) -> Configuration<E, I, L, B, O, FingerprintLegacy<EXPECTED>, FO> {
        generate()
    }

    /// Sets the format to CBOR with default options.
    #[must_use]
    pub const fn with_cbor_format(self) -> Configuration<E, I, L, B, O, F, CborFormat> {
        generate()
    }

    /// Sets the format to CBOR with deterministic encoding (Core mode).
    #[must_use]
    pub const fn with_deterministic_cbor(
        self
    ) -> Configuration<E, I, L, B, O, F, CborDeterministicFormat> {
        generate()
    }

    /// Sets the format to Bincode with deterministic encoding.
    #[must_use]
    pub const fn with_deterministic_bincode(
        self
    ) -> Configuration<E, I, L, B, O, F, BincodeDeterministicFormat> {
        generate()
    }

    /// Sets the format to CBOR with custom options.
    #[must_use]
    pub const fn with_cbor_options<
        const DET: u8,
        const PREF: bool,
        const NAN: bool,
        const NZ: bool,
        const INDEF: bool,
        const TAGS: bool,
    >(
        self
    ) -> Configuration<E, I, L, B, O, F, CborConfig<DET, PREF, NAN, NZ, INDEF, TAGS>> {
        generate()
    }
}

/// Indicates a type is valid for controlling the bincode configuration
pub trait Config:
    InternalEndianConfig
    + InternalIntEncodingConfig
    + InternalLimitConfig
    + InternalBitPackingConfig
    + InternalBitOrderingConfig
    + InternalFingerprintConfig
    + InternalFormatConfig
    + InternalConfigFingerprint
    + InternalFingerprintConfigExt
    + Copy
    + Clone
{
    /// This configuration's Endianness
    fn endianness(&self) -> Endianness;

    /// This configuration's Integer Encoding
    fn int_encoding(&self) -> IntEncoding;

    /// This configuration's byte limit, or `None` if no limit is configured
    fn limit(&self) -> Option<usize>;

    /// Whether bit-packing is enabled for this configuration
    fn bit_packing_enabled(&self) -> bool;

    /// Returns the bit ordering of this configuration
    fn bit_ordering(&self) -> BitOrdering;

    /// Returns the fingerprint mode of this configuration
    fn fingerprint_mode(&self) -> FingerprintMode;

    /// Returns the format of this configuration
    fn format(&self) -> Format;

    /// Returns the CBOR options of this configuration.
    /// Returns default options if the format is not CBOR.
    fn cbor_options(&self) -> CborOptions;

    /// Returns true if this configuration uses compact network encoding (omitting `flowinfo/scope_id`).
    fn is_compact_net(&self) -> bool;
}

impl<T> Config for T
where
    T: InternalEndianConfig
        + InternalIntEncodingConfig
        + InternalLimitConfig
        + InternalBitPackingConfig
        + InternalBitOrderingConfig
        + InternalFingerprintConfig
        + InternalFormatConfig
        + InternalConfigFingerprint
        + InternalFingerprintConfigExt
        + Copy
        + Clone,
{
    fn endianness(&self) -> Endianness {
        <T as InternalEndianConfig>::ENDIAN
    }

    fn int_encoding(&self) -> IntEncoding {
        <T as InternalIntEncodingConfig>::INT_ENCODING
    }

    fn limit(&self) -> Option<usize> {
        <T as InternalLimitConfig>::LIMIT
    }

    fn bit_packing_enabled(&self) -> bool {
        matches!(
            <T as InternalBitPackingConfig>::BIT_PACKING,
            BitPacking::Enabled
        )
    }

    fn bit_ordering(&self) -> BitOrdering {
        <T as InternalBitOrderingConfig>::BIT_ORDERING
    }

    fn fingerprint_mode(&self) -> FingerprintMode {
        <T as InternalFingerprintConfig>::FINGERPRINT_MODE
    }

    fn format(&self) -> Format {
        <T as InternalFormatConfig>::FORMAT
    }

    fn cbor_options(&self) -> CborOptions {
        <T as InternalFormatConfig>::CBOR_OPTIONS
    }

    fn is_compact_net(&self) -> bool {
        <T as InternalFormatConfig>::IS_COMPACT_NET
    }
}

/// Encodes all integer types in big endian.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct BigEndian;

impl InternalEndianConfig for BigEndian {
    const ENDIAN: Endianness = Endianness::Big;
}

/// Encodes all integer types in little endian.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct LittleEndian;

impl InternalEndianConfig for LittleEndian {
    const ENDIAN: Endianness = Endianness::Little;
}

/// Use fixed-size integer encoding.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct Fixint;

impl InternalIntEncodingConfig for Fixint {
    const INT_ENCODING: IntEncoding = IntEncoding::Fixed;
}

/// Use variable integer encoding.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct Varint;

impl InternalIntEncodingConfig for Varint {
    const INT_ENCODING: IntEncoding = IntEncoding::Variable;
}

/// Sets an unlimited byte limit.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct NoLimit;
impl InternalLimitConfig for NoLimit {
    const LIMIT: Option<usize> = None;
}

/// Sets the byte limit to N.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct Limit<const N: usize>;
impl<const N: usize> InternalLimitConfig for Limit<N> {
    const LIMIT: Option<usize> = Some(N);
}

/// Use bit-packing for types that support it.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct AllowBitPacking;

impl InternalBitPackingConfig for AllowBitPacking {
    const BIT_PACKING: BitPacking = BitPacking::Enabled;
}

/// Skip bit-packing.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct SkipBitPacking;

impl InternalBitPackingConfig for SkipBitPacking {
    const BIT_PACKING: BitPacking = BitPacking::Disabled;
}

/// Use MSB-first bit ordering.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct MsbFirst;

impl InternalBitOrderingConfig for MsbFirst {
    const BIT_ORDERING: BitOrdering = BitOrdering::Msb;
}

/// Use LSB-first bit ordering.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct LsbFirst;

impl InternalBitOrderingConfig for LsbFirst {
    const BIT_ORDERING: BitOrdering = BitOrdering::Lsb;
}

/// Fingerprinting is disabled.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct FingerprintDisabled;

impl InternalFingerprintConfig for FingerprintDisabled {
    const FINGERPRINT_MODE: FingerprintMode = FingerprintMode::Disabled;
}

/// Fingerprinting is enabled with a seed.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct FingerprintEnabled<const SEED: u64>;

impl<const SEED: u64> InternalFingerprintConfig for FingerprintEnabled<SEED> {
    const FINGERPRINT_MODE: FingerprintMode = FingerprintMode::Enabled { seed: SEED };
}

/// Legacy fingerprinting with an expected hash.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct FingerprintLegacy<const EXPECTED: u64>;

impl<const EXPECTED: u64> InternalFingerprintConfig for FingerprintLegacy<EXPECTED> {
    const FINGERPRINT_MODE: FingerprintMode = FingerprintMode::Legacy { expected: EXPECTED };
}

/// The default bincode format.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct BincodeFormat;

impl InternalFormatConfig for BincodeFormat {
    const CBOR_OPTIONS: CborOptions = CborOptions::DEFAULT;
    const FORMAT: Format = Format::Bincode;
    const IS_COMPACT_NET: bool = false;
}

/// Bincode format with deterministic encoding.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct BincodeDeterministicFormat;

impl InternalFormatConfig for BincodeDeterministicFormat {
    const CBOR_OPTIONS: CborOptions = CborOptions::DEFAULT;
    const FORMAT: Format = Format::BincodeDeterministic;
    const IS_COMPACT_NET: bool = false;
}

/// CBOR configuration with const generic options.
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct CborConfig<
    const DET: u8,
    const PREF: bool,
    const NAN: bool,
    const NZ: bool,
    const INDEF: bool,
    const TAGS: bool,
>;

impl<
    const DET: u8,
    const PREF: bool,
    const NAN: bool,
    const NZ: bool,
    const INDEF: bool,
    const TAGS: bool,
> InternalFormatConfig for CborConfig<DET, PREF, NAN, NZ, INDEF, TAGS>
{
    const CBOR_OPTIONS: CborOptions = CborOptions {
        deterministic_mode: match DET {
            | 1 => CborDeterministicMode::Core,
            | 2 => CborDeterministicMode::LengthFirst,
            | _ => CborDeterministicMode::None,
        },
        preferred_float: PREF,
        canonical_nan: NAN,
        normalize_neg_zero: NZ,
        allow_indefinite: INDEF,
        strict_tags: TAGS,
    };
    const FORMAT: Format = if DET == 0 {
        Format::Cbor
    } else {
        Format::CborDeterministic
    };
    const IS_COMPACT_NET: bool = false;
}

/// Bincode format for legacy compatibility (omitting flowinfo/scope_id).
#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
pub struct BincodeLegacyFormat;

impl InternalFormatConfig for BincodeLegacyFormat {
    const CBOR_OPTIONS: CborOptions = CborOptions::DEFAULT;
    const FORMAT: Format = Format::Bincode;
    const IS_COMPACT_NET: bool = true;
}

/// CBOR format with default options.
#[doc(hidden)]
pub type CborFormat = CborConfig<0, true, true, false, true, false>;

/// CBOR format with deterministic encoding (Canonical CBOR).
#[doc(hidden)]
pub type CborDeterministicFormat = CborConfig<1, true, true, false, false, true>;

/// Endianness of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Endianness {
    /// Little Endian encoding, see `LittleEndian`.
    Little,
    /// Big Endian encoding, see `BigEndian`.
    Big,
}

/// Integer Encoding of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntEncoding {
    /// Fixed Integer Encoding, see `Fixint`.
    Fixed,
    /// Variable Integer Encoding, see `Varint`.
    Variable,
}

/// Bit packing of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BitPacking {
    /// Enable bit-packing.
    Enabled,
    /// Disable bit-packing.
    Disabled,
}

/// Bit ordering of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BitOrdering {
    /// Least Significant Bit first.
    Lsb,
    /// Most Significant Bit first.
    Msb,
}

/// Fingerprint mode of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FingerprintMode {
    /// Fingerprinting is disabled.
    Disabled,
    /// Fingerprinting is enabled with a seed.
    Enabled {
        /// The seed to use for hashing.
        seed: u64,
    },
    /// Legacy fingerprinting with an expected hash.
    Legacy {
        /// The hash that is expected to be present.
        expected: u64,
    },
}

/// Format of a `Configuration`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// Bincode (default).
    Bincode,
    /// Bincode with deterministic encoding.
    BincodeDeterministic,
    /// CBOR.
    Cbor,
    /// CBOR with deterministic encoding.
    CborDeterministic,
}

#[doc(hidden)]
pub mod internal {
    use super::BitOrdering;
    use super::BitPacking;
    use super::Configuration;
    use super::Endianness;
    use super::FingerprintMode;
    use super::IntEncoding;

    pub trait InternalEndianConfig {
        const ENDIAN: Endianness;
    }

    impl<E: InternalEndianConfig, I, L, B, O, F, FO> InternalEndianConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const ENDIAN: Endianness = E::ENDIAN;
    }

    pub trait InternalIntEncodingConfig {
        const INT_ENCODING: IntEncoding;
    }

    impl<E, I: InternalIntEncodingConfig, L, B, O, F, FO> InternalIntEncodingConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const INT_ENCODING: IntEncoding = I::INT_ENCODING;
    }

    pub trait InternalLimitConfig {
        const LIMIT: Option<usize>;
    }

    impl<E, I, L: InternalLimitConfig, B, O, F, FO> InternalLimitConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const LIMIT: Option<usize> = L::LIMIT;
    }

    pub trait InternalBitPackingConfig {
        const BIT_PACKING: BitPacking;
    }

    impl<E, I, L, B: InternalBitPackingConfig, O, F, FO> InternalBitPackingConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const BIT_PACKING: BitPacking = B::BIT_PACKING;
    }

    pub trait InternalBitOrderingConfig {
        const BIT_ORDERING: BitOrdering;
    }

    impl<E, I, L, B, O: InternalBitOrderingConfig, F, FO> InternalBitOrderingConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const BIT_ORDERING: BitOrdering = O::BIT_ORDERING;
    }

    pub trait InternalFingerprintConfig {
        const FINGERPRINT_MODE: FingerprintMode;
    }

    impl<E, I, L, B, O, F: InternalFingerprintConfig, FO> InternalFingerprintConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const FINGERPRINT_MODE: FingerprintMode = F::FINGERPRINT_MODE;
    }

    pub trait InternalFingerprintConfigExt {
        type Mode;
    }

    impl<E, I, L, B, O, F, FO> InternalFingerprintConfigExt for Configuration<E, I, L, B, O, F, FO> {
        type Mode = F;
    }

    pub trait InternalFingerprintGuard<D, C: super::Config> {
        fn decode_check<R: crate::de::read::Reader>(
            _config: &C,
            reader: &mut R,
        ) -> core::result::Result<(), crate::error::DecodeError>;
        fn encode_check<W: crate::enc::write::Writer>(
            _config: &C,
            writer: &mut W,
        ) -> core::result::Result<(), crate::error::EncodeError>;
    }

    impl<D, C: super::Config> InternalFingerprintGuard<D, C> for super::FingerprintDisabled {
        #[inline(always)]
        fn decode_check<R: crate::de::read::Reader>(
            _config: &C,
            _reader: &mut R,
        ) -> core::result::Result<(), crate::error::DecodeError> {
            core::result::Result::Ok(())
        }

        #[inline(always)]
        fn encode_check<W: crate::enc::write::Writer>(
            _config: &C,
            _writer: &mut W,
        ) -> core::result::Result<(), crate::error::EncodeError> {
            core::result::Result::Ok(())
        }
    }

    impl<D: crate::fingerprint::Fingerprint<C>, C: super::Config, const SEED: u64>
        InternalFingerprintGuard<D, C> for super::FingerprintEnabled<SEED>
    {
        #[inline]
        fn decode_check<R: crate::de::read::Reader>(
            _config: &C,
            reader: &mut R,
        ) -> core::result::Result<(), crate::error::DecodeError> {
            let mut bytes = [0u8; 8];
            reader.read(&mut bytes)?;
            let actual = u64::from_le_bytes(bytes);
            if actual != D::SCHEMA_HASH {
                return crate::error::cold_decode_error_schema_mismatch(D::SCHEMA_HASH, actual);
            }
            core::result::Result::Ok(())
        }

        #[inline]
        fn encode_check<W: crate::enc::write::Writer>(
            _config: &C,
            writer: &mut W,
        ) -> core::result::Result<(), crate::error::EncodeError> {
            writer.write(&D::SCHEMA_HASH.to_le_bytes())
        }
    }

    impl<D, C: super::Config, const EXPECTED: u64> InternalFingerprintGuard<D, C>
        for super::FingerprintLegacy<EXPECTED>
    {
        #[inline]
        fn decode_check<R: crate::de::read::Reader>(
            _config: &C,
            reader: &mut R,
        ) -> core::result::Result<(), crate::error::DecodeError> {
            let mut bytes = [0u8; 8];
            reader.read(&mut bytes)?;
            let actual = u64::from_le_bytes(bytes);
            if actual != EXPECTED {
                return crate::error::cold_decode_error_schema_mismatch(EXPECTED, actual);
            }
            core::result::Result::Ok(())
        }

        #[inline]
        fn encode_check<W: crate::enc::write::Writer>(
            _config: &C,
            _writer: &mut W,
        ) -> core::result::Result<(), crate::error::EncodeError> {
            core::result::Result::Ok(())
        }
    }

    pub trait InternalConfigFingerprint {
        const CONFIG_HASH: u64;
    }

    impl<E, I, L, B, O, F, FO> InternalConfigFingerprint for Configuration<E, I, L, B, O, F, FO>
    where
        E: InternalEndianConfig,
        I: InternalIntEncodingConfig,
        L: InternalLimitConfig,
        B: InternalBitPackingConfig,
        O: InternalBitOrderingConfig,
        FO: InternalFormatConfig,
    {
        const CONFIG_HASH: u64 = {
            let format_byte: u8 = match FO::FORMAT {
                | super::Format::Bincode => 0,
                | super::Format::BincodeDeterministic => 1,
                | super::Format::Cbor => 2,
                | super::Format::CborDeterministic => 3,
            };
            let opts = FO::CBOR_OPTIONS;
            rapidhash::v3::rapidhash_v3_seeded(
                &[
                    crate::BINCODE_MAJOR_VERSION as u8,
                    E::ENDIAN as u8,
                    I::INT_ENCODING as u8,
                    match L::LIMIT {
                        | None => 0,
                        | Some(_) => 1,
                    },
                    B::BIT_PACKING as u8,
                    O::BIT_ORDERING as u8,
                    format_byte,
                    opts.deterministic_mode as u8,
                    opts.preferred_float as u8,
                    opts.canonical_nan as u8,
                    opts.normalize_neg_zero as u8,
                    opts.allow_indefinite as u8,
                    opts.strict_tags as u8,
                ],
                &rapidhash::v3::RapidSecrets::seed_cpp(0),
            )
        };
    }

    pub trait InternalFormatConfig {
        const FORMAT: super::Format;
        const CBOR_OPTIONS: super::CborOptions;
        const IS_COMPACT_NET: bool;
    }

    impl<E, I, L, B, O, F, FO: InternalFormatConfig> InternalFormatConfig
        for Configuration<E, I, L, B, O, F, FO>
    {
        const CBOR_OPTIONS: super::CborOptions = FO::CBOR_OPTIONS;
        const FORMAT: super::Format = FO::FORMAT;
        const IS_COMPACT_NET: bool = FO::IS_COMPACT_NET;
    }
}
