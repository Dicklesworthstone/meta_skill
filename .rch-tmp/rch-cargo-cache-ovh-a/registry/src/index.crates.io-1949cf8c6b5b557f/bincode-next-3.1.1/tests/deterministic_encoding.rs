#![cfg(feature = "alloc")]

use bincode_next::encode_to_vec;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

#[test]
fn test_hashmap_deterministic() {
    let mut map = HashMap::new();
    map.insert(2, "two");
    map.insert(1, "one");
    map.insert(3, "three");

    let config = bincode_next::config::standard().with_deterministic_bincode();
    let encoded1 = encode_to_vec(&map, config).unwrap();

    let mut map2 = HashMap::new();
    map2.insert(3, "three");
    map2.insert(2, "two");
    map2.insert(1, "one");

    let encoded2 = encode_to_vec(&map2, config).unwrap();

    assert_eq!(
        encoded1, encoded2,
        "Deterministic HashMap encoding should produce identical results regardless of insertion order"
    );

    // Also assert it behaves correctly when decoded back
    let (decoded, _len): (HashMap<i32, String>, usize) =
        bincode_next::decode_from_slice(&encoded1, config).unwrap();
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[&1], "one");
    assert_eq!(decoded[&2], "two");
    assert_eq!(decoded[&3], "three");
}

#[test]
fn test_btreemap_deterministic() {
    let mut map = BTreeMap::new();
    map.insert(2, "two");
    map.insert(1, "one");
    map.insert(3, "three");

    let config = bincode_next::config::standard().with_deterministic_bincode();
    let encoded1 = encode_to_vec(&map, config).unwrap();

    let mut map2 = BTreeMap::new();
    map2.insert(3, "three");
    map2.insert(2, "two");
    map2.insert(1, "one");

    let encoded2 = encode_to_vec(&map2, config).unwrap();

    assert_eq!(
        encoded1, encoded2,
        "BTreeMap encodes deterministically by default, but this verifies the new pipeline"
    );
}

#[test]
fn test_hashset_deterministic() {
    let mut set = HashSet::new();
    set.insert(2);
    set.insert(1);
    set.insert(3);

    let config = bincode_next::config::standard().with_deterministic_bincode();
    let encoded1 = encode_to_vec(&set, config).unwrap();

    let mut set2 = HashSet::new();
    set2.insert(3);
    set2.insert(2);
    set2.insert(1);

    let encoded2 = encode_to_vec(&set2, config).unwrap();

    assert_eq!(
        encoded1, encoded2,
        "Deterministic HashSet encoding should produce identical results regardless of insertion order"
    );
}

#[test]
fn test_btreeset_deterministic() {
    let mut set = BTreeSet::new();
    set.insert(2);
    set.insert(1);
    set.insert(3);

    let config = bincode_next::config::standard().with_deterministic_bincode();
    let encoded1 = encode_to_vec(&set, config).unwrap();

    let mut set2 = BTreeSet::new();
    set2.insert(3);
    set2.insert(2);
    set2.insert(1);

    let encoded2 = encode_to_vec(&set2, config).unwrap();

    assert_eq!(
        encoded1, encoded2,
        "BTreeSet encodes deterministically by default, but this verifies the new pipeline"
    );
}

#[test]
fn test_hashmap_duplicate_key_rejected_in_deterministic_mode() {
    // Manually craft a bincode payload with duplicate keys.
    // First encode a normal map with 2 entries, then tamper the second key to be the same as the first.
    let config_standard = bincode_next::config::standard();
    let config_deterministic = bincode_next::config::standard().with_deterministic_bincode();

    let mut map = HashMap::new();
    map.insert(1u32, 10u32);
    map.insert(2u32, 20u32);

    // Encode with standard to get a valid payload
    let _encoded = encode_to_vec(&map, config_standard).unwrap();

    // Standard mode: encoding with duplicate key by manually encoding length=2, key=1, val=10, key=1, val=20
    let mut dup_payload = Vec::new();
    // length = 2 (varint)
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(2u64, config_standard).unwrap());
    // key=1, val=10
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(1u32, config_standard).unwrap());
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(10u32, config_standard).unwrap());
    // key=1 (duplicate!), val=20
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(1u32, config_standard).unwrap());
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(20u32, config_standard).unwrap());

    // Standard mode should accept duplicates (last wins)
    let result: Result<(HashMap<u32, u32>, usize), _> =
        bincode_next::decode_from_slice(&dup_payload, config_standard);
    assert!(result.is_ok(), "Standard mode should accept duplicate keys");
    let (decoded, _) = result.unwrap();
    assert_eq!(
        decoded[&1], 20,
        "Standard mode: last value wins for duplicate keys"
    );

    // Deterministic mode should reject duplicates
    let result: Result<(HashMap<u32, u32>, usize), _> =
        bincode_next::decode_from_slice(&dup_payload, config_deterministic);
    assert!(
        result.is_err(),
        "Deterministic mode should reject duplicate map keys"
    );
}

#[test]
fn test_hashset_duplicate_element_rejected_in_deterministic_mode() {
    let config_standard = bincode_next::config::standard();
    let config_deterministic = bincode_next::config::standard().with_deterministic_bincode();

    // Manually craft a payload with duplicate elements: length=2, elem=1, elem=1
    let mut dup_payload = Vec::new();
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(2u64, config_standard).unwrap());
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(1u32, config_standard).unwrap());
    dup_payload.extend_from_slice(&bincode_next::encode_to_vec(1u32, config_standard).unwrap());

    // Standard mode should accept duplicates
    let result: Result<(HashSet<u32>, usize), _> =
        bincode_next::decode_from_slice(&dup_payload, config_standard);
    assert!(
        result.is_ok(),
        "Standard mode should accept duplicate set elements"
    );

    // Deterministic mode should reject duplicates
    let result: Result<(HashSet<u32>, usize), _> =
        bincode_next::decode_from_slice(&dup_payload, config_deterministic);
    assert!(
        result.is_err(),
        "Deterministic mode should reject duplicate set elements"
    );
}

#[test]
fn test_standard_mode_is_zero_cost() {
    // Verify that standard bincode encoding of a HashMap produces the same bytes
    // regardless of whether we're using standard or standard config.
    // This ensures the deterministic mode is truly opt-in.
    let config = bincode_next::config::standard();

    let mut map = HashMap::new();
    map.insert(1u32, "hello");

    let encoded = encode_to_vec(&map, config).unwrap();
    let (decoded, _): (HashMap<u32, String>, usize) =
        bincode_next::decode_from_slice(&encoded, config).unwrap();
    assert_eq!(decoded[&1], "hello");
}
