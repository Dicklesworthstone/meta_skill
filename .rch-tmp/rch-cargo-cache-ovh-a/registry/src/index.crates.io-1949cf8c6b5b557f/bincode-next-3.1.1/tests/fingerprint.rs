#![cfg(feature = "derive")]
#![cfg(feature = "std")]

use bincode_next::Decode;
use bincode_next::Encode;
use bincode_next::Fingerprint;
use bincode_next::config;
use bincode_next::decode_from_slice;
use bincode_next::encode_into_slice;

#[derive(Fingerprint, Encode, Decode, PartialEq, Debug, Clone)]
struct MyStruct {
    a: u32,
    b: String,
}

#[derive(Fingerprint, Encode, Decode, PartialEq, Debug, Clone)]
struct MyStructV2 {
    a: u32,
    b: String,
    c: u32,
}

#[derive(Fingerprint, Encode, Decode, PartialEq, Debug, Clone)]
struct MyStructNameChange {
    a: u32,
    different_name: String,
}

#[test]
fn test_fingerprint_basic() {
    let mut buf = [0u8; 100];
    let val = MyStruct {
        a: 1,
        b: "hello".to_string(),
    };
    let config = config::standard().with_fingerprint();

    let len = encode_into_slice(val.clone(), &mut buf, config).unwrap();

    fn hash<C: config::Config, T: Fingerprint<C>>(_: C) -> u64 {
        T::SCHEMA_HASH
    }
    let hash = hash::<_, MyStruct>(config);
    assert_eq!(&buf[0..8], &hash.to_le_bytes());

    let (decoded, decoded_len): (MyStruct, _) = decode_from_slice(&buf[..len], config).unwrap();

    assert_eq!(val, decoded);
    assert_eq!(len, decoded_len);
}

#[test]
fn test_fingerprint_mismatch() {
    let mut buf = [0u8; 100];
    let val = MyStruct {
        a: 1,
        b: "hello".to_string(),
    };
    let config = config::standard().with_fingerprint();

    let len = encode_into_slice(val, &mut buf, config).unwrap();

    let res: Result<(MyStructV2, usize), _> = decode_from_slice(&buf[..len], config);
    match res {
        | Err(bincode_next::error::DecodeError::SchemaMismatch { expected, actual }) => {
            fn hash<C: config::Config, T: Fingerprint<C>>(_: C) -> u64 {
                T::SCHEMA_HASH
            }
            assert_eq!(expected, hash::<_, MyStructV2>(config));
            assert_eq!(actual, hash::<_, MyStruct>(config));
        },
        | _ => panic!("Expected SchemaMismatch, got {:?}", res),
    }
}

#[test]
fn test_fingerprint_name_change_mismatch() {
    let mut buf = [0u8; 100];
    let val = MyStruct {
        a: 1,
        b: "hello".to_string(),
    };
    let config = config::standard().with_fingerprint();

    let len = encode_into_slice(val, &mut buf, config).unwrap();

    let res: Result<(MyStructNameChange, usize), _> = decode_from_slice(&buf[..len], config);
    match res {
        | Err(bincode_next::error::DecodeError::SchemaMismatch { .. }) => {},
        | _ => panic!("Expected SchemaMismatch because of field name change"),
    }
}

#[test]
fn test_fingerprint_config_change() {
    let config_le = config::standard().with_little_endian().with_fingerprint();
    let config_be = config::standard().with_big_endian().with_fingerprint();

    fn hash<C: config::Config, T: Fingerprint<C>>(_: C) -> u64 {
        T::SCHEMA_HASH
    }

    let hash_le = hash::<_, MyStruct>(config_le);
    let hash_be = hash::<_, MyStruct>(config_be);

    assert_ne!(
        hash_le, hash_be,
        "Hash should change with config (endianness)"
    );
}

#[test]
fn test_fingerprint_int_encoding_change() {
    let config_var = config::standard()
        .with_variable_int_encoding()
        .with_fingerprint();
    let config_fix = config::standard()
        .with_fixed_int_encoding()
        .with_fingerprint();

    fn hash<C: config::Config, T: Fingerprint<C>>(_: C) -> u64 {
        T::SCHEMA_HASH
    }

    let hash_var = hash::<_, MyStruct>(config_var);
    let hash_fix = hash::<_, MyStruct>(config_fix);

    assert_ne!(
        hash_var, hash_fix,
        "Hash should change with config (int encoding)"
    );
}

#[test]
fn test_fingerprint_legacy() {
    let mut buf = [0u8; 100];
    let val = MyStruct {
        a: 1,
        b: "hello".to_string(),
    };
    let config = config::standard().with_fingerprint();

    let len = encode_into_slice(val.clone(), &mut buf, config).unwrap();
    const EXPECTED_HASH: u64 = <MyStruct as Fingerprint<config::Configuration>>::SCHEMA_HASH;

    let legacy_config = config::standard().with_legacy_fingerprint::<0>(); // Wrong hash
    let res: Result<(MyStruct, usize), _> = decode_from_slice(&buf[..len], legacy_config);
    match res {
        | Err(bincode_next::error::DecodeError::SchemaMismatch { expected, actual }) => {
            assert_eq!(expected, 0);
            assert_eq!(actual, EXPECTED_HASH);
        },
        | _ => panic!("Expected SchemaMismatch for legacy with wrong hash"),
    }

    let legacy_config_correct = config::standard().with_legacy_fingerprint::<EXPECTED_HASH>();
    let (decoded, _): (MyStruct, _) =
        decode_from_slice(&buf[..len], legacy_config_correct).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_backward_compatibility() {
    let mut buf = [0u8; 100];
    let val = MyStruct {
        a: 1,
        b: "hello".to_string(),
    };
    let config = config::standard(); // No fingerprint

    let len = encode_into_slice(val.clone(), &mut buf, config).unwrap();

    // First byte should NOT be the hash. It should be 'a' (1) encoded as u8 or varint.
    // u32 1 in varint is 1.
    assert_eq!(buf[0], 1);

    let (decoded, _): (MyStruct, _) = decode_from_slice(&buf[..len], config).unwrap();
    assert_eq!(val, decoded);
}
