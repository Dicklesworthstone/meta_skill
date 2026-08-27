#![cfg(feature = "static-size")]
#![cfg(feature = "derive")]

mod tests {
    use bincode_next::Decode;
    use bincode_next::Encode;
    use bincode_next::StaticSize;
    use bincode_next::bounded::BoundedString;
    use bincode_next::bounded::BoundedVec;
    use bincode_next::decode_from_slice_static;

    #[derive(StaticSize, Encode, Decode, PartialEq, Debug)]
    struct TestStruct {
        a: u32,
        b: u64,
    }

    #[derive(StaticSize, Encode, Decode, PartialEq, Debug)]
    enum TestEnum {
        A(u32),
        B { x: u64, y: u64 },
    }

    #[test]
    fn test_static_size() {
        assert_eq!(<u8 as StaticSize>::MAX_SIZE, 1);
        assert_eq!(<u32 as StaticSize>::MAX_SIZE, 5);
        assert_eq!(<u64 as StaticSize>::MAX_SIZE, 9);
        assert_eq!(TestStruct::MAX_SIZE, 5 + 9);
        // TestEnum: max(5, 9 + 9) + 5 = 18 + 5 = 23
        assert_eq!(TestEnum::MAX_SIZE, 23);
    }

    #[test]
    fn test_decode_static() {
        let val = TestStruct { a: 1, b: 2 };
        let buf = [0u8; 14];
        let mut actual_buf = buf;
        let _ = bincode_next::encode_into_slice(
            &val,
            &mut actual_buf,
            bincode_next::config::standard(),
        )
        .unwrap();

        let decoded: TestStruct =
            decode_from_slice_static(&actual_buf, bincode_next::config::standard()).unwrap();
        assert_eq!(val, decoded);
    }

    #[test]
    fn test_bounded_types() {
        // BoundedVec<u8, 10>: 1 * 10 + 9 = 19
        assert_eq!(BoundedVec::<u8, 10>::MAX_SIZE, 19);

        // BoundedString<10>: 10 + 9 = 19
        assert_eq!(BoundedString::<10>::MAX_SIZE, 19);
    }
}
