#![cfg(feature = "zero-copy")]
use bincode_next::ZeroCopy;
use bincode_next::relative_ptr::RelativePtr;
use bincode_next::relative_ptr::ZeroArray;
use bincode_next::relative_ptr::ZeroSlice;
use bincode_next::relative_ptr::ZeroStr;
use bincode_next::relative_ptr::ZeroString;

#[repr(C)]
#[derive(ZeroCopy)]
struct Nested {
    a: u32,
    b: RelativePtr<u32, 4>,
}

#[repr(C)]
#[derive(ZeroCopy)]
struct Complex {
    str_inline: ZeroString<10>,
    str_relative: ZeroStr,
    slice: ZeroSlice<u32, 8>,
    array: ZeroArray<u32, 4, 16>,
    nested: RelativePtr<Nested, 4>,
}

#[test]
fn test_complex_derive() {
    use bincode_next::relative_ptr::ArrayBuilder;
    use bincode_next::relative_ptr::FixedString;
    use bincode_next::relative_ptr::RelativeBuilder;
    use bincode_next::relative_ptr::SliceBuilder;
    use bincode_next::relative_ptr::ZeroBuilder;
    use bincode_next::relative_ptr::ZeroCopyBuilder;

    let mut builder = ZeroBuilder::new();

    let nested_builder = NestedBuilder {
        a: 100,
        b: RelativeBuilder::<u32, 4>(200),
    };

    let complex_builder = ComplexBuilder {
        str_inline: FixedString("inline".to_string()),
        str_relative: "relative".to_string(),
        slice: SliceBuilder::<u32, 8>(vec![1, 2, 3]),
        array: ArrayBuilder::<u32, 4, 16>([10, 20, 30, 40]),
        nested: RelativeBuilder::<NestedBuilder, 4>(nested_builder),
    };

    let offset = complex_builder.build(&mut builder);
    let buffer = builder.finish();

    let complex = unsafe { &*(buffer.as_ptr().add(offset) as *const Complex) };

    // Basic validation
    eprintln!("buffer len: {}", buffer.len());
    eprintln!("offset: {}", offset);
    eprintln!("complex size: {}", core::mem::size_of::<Complex>());

    assert_eq!(complex.str_inline.get().unwrap(), "inline");
    assert_eq!(complex.str_relative.get(&buffer).unwrap(), "relative");

    let slice = complex.slice.get(&buffer).unwrap();
    assert_eq!(slice, &[1, 2, 3]);

    eprintln!(
        "array ptr offset bytes: {:?}",
        &buffer[offset + core::mem::offset_of!(Complex, array)
            ..offset
                + core::mem::offset_of!(Complex, array)
                + core::mem::size_of::<ZeroArray<u32, 4, 16>>()]
    );
    eprintln!(
        "array field offset in Complex: {}",
        core::mem::offset_of!(Complex, array)
    );
    eprintln!(
        "ZeroArray size: {}",
        core::mem::size_of::<ZeroArray<u32, 4, 16>>()
    );

    let array = complex.array.get(&buffer);
    eprintln!("array result: {:?}", array);
    let array = array.unwrap();
    assert_eq!(array, &[10, 20, 30, 40]);

    let nested_ptr = complex.nested.get(&buffer).unwrap();
    assert_eq!(nested_ptr.a, 100);

    let nested_inner_val = nested_ptr.b.get(&buffer).unwrap();
    assert_eq!(*nested_inner_val, 200);
}
