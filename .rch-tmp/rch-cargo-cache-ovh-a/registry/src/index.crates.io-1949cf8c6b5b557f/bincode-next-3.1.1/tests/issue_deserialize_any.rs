#[cfg(feature = "serde")]
#[test]
fn test_deserialize_any_serde_json() {
    use serde_json::Value;

    let config = bincode_next::config::standard();

    let result: Result<(Value, _), _> = bincode_next::serde::borrow_decode_from_slice(&[0], config);
    println!("{:?}", result);
}
