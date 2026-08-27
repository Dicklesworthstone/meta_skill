#![cfg(feature = "zero-copy")]
#![cfg(feature = "derive")]

use bincode_next::relative_ptr::ZeroBuilder;
use bincode_next::relative_ptr::ZeroCopyBuilder;

#[derive(bincode_derive_next::ZeroCopy, Debug, PartialEq, Eq)]
#[repr(C, u8)]
enum TestEnum {
    A(u32),
    B { x: u64, y: u32 },
    C,
}

#[test]
fn test_zerocopy_enum() {
    let mut builder = ZeroBuilder::new();

    let val_a = TestEnumBuilder::A(42u32);
    let offset_a = builder.reserve::<TestEnum>();
    let target_a = val_a.build_to_target(&mut builder, offset_a);

    // Manual check of target_a
    match target_a {
        | TestEnum::A(v) => assert_eq!(v, 42),
        | _ => panic!("Expected A"),
    }

    let val_b = TestEnumBuilder::B { x: 100u64, y: 200u32 };
    let offset_b = builder.reserve::<TestEnum>();
    let target_b = val_b.build_to_target(&mut builder, offset_b);

    match target_b {
        | TestEnum::B { x, y } => {
            assert_eq!(x, 100);
            assert_eq!(y, 200);
        },
        | _ => panic!("Expected B"),
    }

    let val_c = TestEnumBuilder::C;
    let offset_c = builder.reserve::<TestEnum>();
    let target_c = val_c.build_to_target(&mut builder, offset_c);
    match target_c {
        | TestEnum::C => (),
        | _ => panic!("Expected C"),
    }
}
