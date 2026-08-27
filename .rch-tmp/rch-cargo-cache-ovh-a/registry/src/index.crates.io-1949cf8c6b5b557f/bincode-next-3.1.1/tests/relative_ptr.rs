#![cfg(feature = "zero-copy")]

use bincode_next::relative_ptr::NativeEndian;
use bincode_next::relative_ptr::RelativePtr;
use bincode_next::relative_ptr::Validator;
use bincode_next::relative_ptr::ZeroArray;
use bincode_next::relative_ptr::ZeroString;

#[repr(align(8))]
struct AlignedBuffer<const N: usize>(pub [u8; N]);

#[test]
fn test_relative_ptr_valid() {
    // A buffer with [offset(4 bytes), padding(4 bytes), value(4 bytes) = 42]
    // The offset to the value is 8 bytes.
    let mut buffer = AlignedBuffer([0; 12]);
    let buffer = &mut buffer.0;

    // Write offset: 8 (little endian or native, let's use native for test)
    let offset: i32 = 8;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    // Write target data (42u32) at offset 8
    let value: u32 = 42;
    buffer[8..12].copy_from_slice(&value.to_ne_bytes());

    // Cast the first 4 bytes to RelativePtr
    let ptr = unsafe { &*(buffer.as_ptr() as *const RelativePtr<u32, 4>) };

    assert!(ptr.is_valid(&buffer[..]));
    let resolved = ptr.get(&buffer[..]).unwrap();
    assert_eq!(*resolved, 42);
}

#[test]
fn test_relative_ptr_negative_offset() {
    let mut buffer = AlignedBuffer([0; 12]);
    let buffer = &mut buffer.0;

    // Write value first: 42 at offset 0
    let value: u32 = 42;
    buffer[0..4].copy_from_slice(&value.to_ne_bytes());

    // Write negative offset: -8 at offset 8
    let offset: i32 = -8;
    buffer[8..12].copy_from_slice(&offset.to_ne_bytes());

    let ptr = unsafe { &*(buffer[8..].as_ptr() as *const RelativePtr<u32, 4>) };

    assert!(ptr.is_valid(&buffer[..]));
    let resolved = ptr.get(&buffer[..]).unwrap();
    assert_eq!(*resolved, 42);
}

#[test]
fn test_relative_ptr_out_of_bounds() {
    let mut buffer = AlignedBuffer([0; 8]);
    let buffer = &mut buffer.0;

    // Offset that points outside the buffer
    let offset: i32 = 12;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    let ptr = unsafe { &*(buffer.as_ptr() as *const RelativePtr<u32, 4>) };

    assert!(!ptr.is_valid(&buffer[..]));
    assert!(ptr.get(&buffer[..]).is_none());
}

#[test]
fn test_relative_ptr_negative_out_of_bounds() {
    let mut buffer = AlignedBuffer([0; 8]);
    let buffer = &mut buffer.0;

    // Offset that points before the buffer (negative)
    let offset: i32 = -4;
    buffer[4..8].copy_from_slice(&offset.to_ne_bytes()); // ptr is at offset 4

    // Buffer boundary checks should fail
    let ptr = unsafe { &*(buffer[4..].as_ptr() as *const RelativePtr<u32, 4>) };

    // target is at buffer pointer - 4? No, it's at offset 0, wait!
    // ptr is at buffer + 4. offset -4 means target is at buffer + 0.
    // So target is inside buffer! Let's check:
    let resolved = ptr.get(&buffer[..]).unwrap();
    assert_eq!(*resolved, 0); // 0 because we didn't write anything

    // Now truly out of bounds negative
    let offset2: i32 = -8;
    buffer[4..8].copy_from_slice(&offset2.to_ne_bytes());
    let ptr2 = unsafe { &*(buffer[4..].as_ptr() as *const RelativePtr<u32, 4>) };
    assert!(ptr2.get(&buffer[..]).is_none());
}

#[test]
fn test_relative_ptr_bad_alignment() {
    // offset 8, but we try to resolve an i32 with align 4 at offset 9
    let mut buffer = AlignedBuffer([0; 16]);
    let buffer = &mut buffer.0;

    // We put ptr at offset 1 to mimic bad starting alignment, but ptr aligns are checked later.
    // To misalign target, we put ptr at offset 0, and offset to target = 5.
    let offset: i32 = 5;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    let ptr = unsafe { &*(buffer.as_ptr() as *const RelativePtr<u32, 4>) };

    // Target is at offset 5, which is not 4-byte aligned.
    assert!(!ptr.is_valid(&buffer[..]));
    assert!(ptr.get(&buffer[..]).is_none());
}

#[test]
fn test_zero_array() {
    let mut buffer = AlignedBuffer([0; 16]);
    let buffer = &mut buffer.0;

    // Write array of [u32; 2] at offset 8 -> values [10, 20]
    let vals: [u32; 2] = [10, 20];
    let vals_bytes = unsafe { core::slice::from_raw_parts(vals.as_ptr() as *const u8, 8) };
    buffer[8..16].copy_from_slice(vals_bytes);

    // Write ptr at offset 0 pointing to offset 8
    let offset: i32 = 8;
    buffer[0..4].copy_from_slice(&offset.to_ne_bytes());

    let arr = unsafe { &*(buffer.as_ptr() as *const ZeroArray<u32, 2, 4>) };

    assert!(arr.is_valid(&buffer[..]));
    let resolved: &[u32; 2] = arr.get(&buffer[..]).unwrap();
    assert_eq!(resolved[0], 10);
    assert_eq!(resolved[1], 20);
}

#[test]
#[cfg(feature = "alloc")]
fn test_zero_string() {
    use bincode_next::relative_ptr::FixedString;
    use bincode_next::relative_ptr::ZeroBuilder;
    use bincode_next::relative_ptr::ZeroCopyBuilder;
    let mut builder = ZeroBuilder::new();
    let text = "hello".to_string();
    let z_builder: FixedString<5> = FixedString(text);
    <FixedString<5> as ZeroCopyBuilder<NativeEndian, 0>>::build(z_builder, &mut builder);
    let buffer = builder.finish();

    let z_str = unsafe { &*(buffer.as_ptr() as *const ZeroString<5, NativeEndian>) };

    assert!(z_str.is_valid(&buffer[..]));
    let resolved = z_str.get().unwrap();
    assert_eq!(resolved, "hello");
}

// Testing Dynamic Size slices

use bincode_next::relative_ptr::ZeroSlice;
use bincode_next::relative_ptr::ZeroStr;

#[test]
fn test_zero_slice() {
    let mut buffer = AlignedBuffer([0; 24]);
    let buffer = &mut buffer.0;

    // offset 0..4: len = 3
    let len: u32 = 3;
    buffer[0..4].copy_from_slice(&len.to_ne_bytes());

    // offset 4..8: offset to data = 8
    let offset: i32 = 8;
    buffer[4..8].copy_from_slice(&offset.to_ne_bytes());

    // Write array of [u32; 3] at offset 12 -> values [100, 200, 300]
    let vals: [u32; 3] = [100, 200, 300];
    let vals_bytes = unsafe { core::slice::from_raw_parts(vals.as_ptr() as *const u8, 12) };
    buffer[12..24].copy_from_slice(vals_bytes);

    let slice = unsafe { &*(buffer.as_ptr() as *const ZeroSlice<u32, 4>) };

    assert!(slice.is_valid(buffer));
    let resolved = slice.get(buffer).unwrap();
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0], 100);
    assert_eq!(resolved[1], 200);
    assert_eq!(resolved[2], 300);
}

#[test]
fn test_zero_str() {
    let mut buffer = AlignedBuffer([0; 16]);
    let buffer = &mut buffer.0;

    let text = b"bincode";

    // len at offset 0
    let len: u32 = 7;
    buffer[0..4].copy_from_slice(&len.to_ne_bytes());

    // offset at offset 4: offset = 4 (points to offset 8)
    let offset: i32 = 4;
    buffer[4..8].copy_from_slice(&offset.to_ne_bytes());

    buffer[8..15].copy_from_slice(text);

    let z_str = unsafe { &*(buffer.as_ptr() as *const ZeroStr) };

    assert!(z_str.is_valid(buffer));
    let resolved = z_str.get(buffer).unwrap();
    assert_eq!(resolved, "bincode");
}

#[test]
fn test_nested_slices() {
    // Nested zero-copy support: ZeroSlice<ZeroSlice<u32, 4>, 8> (Assuming 8 byte struct align)
    let mut buffer = AlignedBuffer([0; 64]);
    let buffer = &mut buffer.0;

    // Let's create an outer slice of 2 elements.
    // Outer Slice Struct at 0..8
    let outer_len: u32 = 2;
    buffer[0..4].copy_from_slice(&outer_len.to_ne_bytes());
    let outer_offset: i32 = 4; // 4 + 4 = offset 8
    buffer[4..8].copy_from_slice(&outer_offset.to_ne_bytes());

    // Element 1 of outer slice (lies at buffer[8..16])
    let inner_1_len: u32 = 1;
    buffer[8..12].copy_from_slice(&inner_1_len.to_ne_bytes());
    let _inner_1_offset: i32 = 16; // From 12 + 16 = offset 28. But wait: RelativePtr is an i32 relative to its OWN address.
    // Address of inner_1_offset is 12. If we put inner array 1 at 32: 32 - 12 = 20
    buffer[12..16].copy_from_slice(&20i32.to_ne_bytes());

    let vals1: u32 = 111;
    buffer[32..36].copy_from_slice(&vals1.to_ne_bytes());

    // Element 2 of outer slice (lies at buffer[16..24])
    let inner_2_len: u32 = 2;
    buffer[16..20].copy_from_slice(&inner_2_len.to_ne_bytes());
    // Address of inner_2_offset is 20. If we put inner array 2 at 40: 40 - 20 = 20
    buffer[20..24].copy_from_slice(&20i32.to_ne_bytes());

    let vals2: [u32; 2] = [222, 333];
    let vals2_bytes = unsafe { core::slice::from_raw_parts(vals2.as_ptr() as *const u8, 8) };
    buffer[40..48].copy_from_slice(vals2_bytes);

    type InnerSlice = ZeroSlice<u32, 4>;
    // The struct `ZeroSlice` has alignment 4 (it's composed of u32 and i32).
    let slice = unsafe { &*(buffer.as_ptr() as *const ZeroSlice<InnerSlice, 4>) };

    assert!(slice.is_valid(buffer));
    let resolved = slice.get(buffer).unwrap();
    assert_eq!(resolved.len(), 2);

    // Retrieve nested elements safely without allocations
    let inner_1 = resolved[0].get(buffer).unwrap();
    assert_eq!(inner_1[0], 111);

    let inner_2 = resolved[1].get(buffer).unwrap();
    assert_eq!(inner_2.len(), 2);
    assert_eq!(inner_2[0], 222);
    assert_eq!(inner_2[1], 333);
}
