# Serialization Specification

_NOTE_: This specification is primarily defined in the context of Rust, but aims to be implementable across different programming languages.

## Definitions

- **Variant**: A specific constructor or case of an enum type.
- **Variant Payload**: The associated data of a specific enum variant.
- **Discriminant**: A unique identifier for an enum variant, typically represented as an integer.
- **Basic Types**: Primitive types that have a direct, well-defined binary representation.

## Endianness

By default, this serialization format uses little-endian byte order for basic numeric types. This means multi-byte values are encoded with their least significant byte first.

Endianness can be configured with the following methods, allowing for big-endian serialization when required:

- [`with_big_endian`](config/struct.Configuration.html#method.with_big_endian)
- [`with_little_endian`](config/struct.Configuration.html#method.with_little_endian)

### Byte Order Considerations

- Multi-byte values (integers, floats) are affected by endianness
- Single-byte values (u8, i8) are not affected
- Struct and collection serialization order is not changed by endianness

## Basic Types

### Boolean Encoding

- Encoded as a single byte
- `false` is represented by `0`
- `true` is represented by `1`
- During deserialization, values other than 0 and 1 will result in an error `DecodeError::InvalidBooleanValue`

### Numeric Types

- Encoded based on the configured [IntEncoding](#intencoding)
- Signed integers use 2's complement representation
- Floating point types use IEEE 754-2008 standard
  - `f32`: 4 bytes (binary32)
  - `f64`: 8 bytes (binary64)

#### Floating Point Special Values

- Subnormal numbers maintenance: Their exact bit representation is preserved.
- `NaN` values: Both quiet and signaling `NaN` are kept as-is, with their bit pattern maintained.

### Character Encoding

- `char` is encoded as a 32-bit unsigned integer representing its Unicode Scalar Value.
- Valid range: 0x0000 to 0xD7FF and 0xE000 to 0x10FFFF.
- Invalid characters encountered during decoding raise `DecodeError::InvalidCharEncoding`.

## IntEncoding

Bincode currently supports 2 different types of `IntEncoding`. With the default config, `VarintEncoding` is selected.

### VarintEncoding

Encoding an unsigned integer `u` works as follows:

1. If `u < 251`, encode it as a single byte with that value.
1. If `251 <= u < 2**16`, encode it as a literal byte 251, followed by a u16 with value `u`.
1. If `2**16 <= u < 2**32`, encode it as a literal byte 252, followed by a u32 with value `u`.
1. If `2**32 <= u < 2**64`, encode it as a literal byte 253, followed by a u64 with value `u`.
1. If `2**64 <= u < 2**128`, encode it as a literal byte 254, followed by a u128 with value `u`.

`usize` is encoded as `u64` and `isize` as `i64`.

### FixintEncoding

- Fixed size integers (u16..u128, i16..i128) are encoded directly in the specified endianness.
- Enum discriminants are encoded as `u32`.
- Lengths and `usize` are encoded as `u64`.

## Bit-Packed Layout

When `BitPacking` is enabled in the configuration, types marked as `BitPacked` (via the derive macro) use a specialized bit-level layout.

### Layout Principles
- Fields are packed into the smallest number of bytes that can contain them.
- Bits are filled from the least significant bit (LSB) to the most significant bit (MSB) of each byte.
- If a field spans a byte boundary, it continues from the LSB of the next byte.
- After all bit-packed fields in a struct/type are encoded, the bit-buffer is flushed, and any remaining bits in the last byte are zero-padded to align to the next byte boundary.

### Supported Fields
- Integers with explicit bit-widths (e.g., `#[bincode(bits = 3)]`).
- `bool` (takes 1 bit).
- Enums (if all variants are unit variants, the discriminant takes `ceil(log2(N))` bits).

## Collections

### General Collection Serialization

Collections are encoded with their length value first, followed by each entry. The length value is based on the configured `IntEncoding`.

### Arrays

Fixed-length array length is **never** encoded. The elements follow strictly in sequence.

## String and &str

- Encoded as UTF-8 byte sequences.
- Length is encoded first using the configured `IntEncoding`, followed by the raw bytes.
- No null terminator or BOM is added.

## Performance Implementation Notes (Internal)

While not affecting the wire format, implementers should consider the following for extreme performance:

1. **SIMD Varint Scanning**: When decoding a `Vec` of variable-length integers, scanning for consecutive single-byte varints (values 0..250) using SIMD (SSE2/NEON) can significantly increase throughput.
2. **Bulk Copy**: If the encoding is `Fixint` and the target system endianness matches the configuration (or for 1-byte types), slices can be copied directly using `memcpy` or equivalent.
3. **Double-Pass Avoidance**: For collections, pre-allocate space (`Vec::with_capacity`) and read directly into uninitialized memory (`MaybeUninit`) to avoid zero-initialization overhead.
