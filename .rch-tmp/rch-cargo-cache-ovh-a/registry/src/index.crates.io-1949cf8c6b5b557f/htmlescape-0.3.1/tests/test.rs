
extern crate htmlescape;

extern crate num;
extern crate rand;

use std::char;

use htmlescape::*;
use htmlescape::DecodeErrKind::*;

macro_rules! assert_typed_eq (($T: ty, $given: expr, $expected: expr) => ({
    let given_val: &$T = $given;
    let expected_val: &$T = $expected;
    assert_eq!(given_val, expected_val);
}));

macro_rules! test_decode_err(($name: ident, $inp: expr, $pos: expr, $kind: expr) => (
        #[test]
        fn $name() {
            match decode_html($inp) {
                Ok(s) => panic!("Expected error, got '{}'", s),
                Err(e) => assert_eq!(DecodeErr{position: $pos, kind: $kind}, e)
            }
        }));

#[test]
fn test_encode_minimal() {
    let data = [
        // (input, expected_output)
        ("", ""),
        ("Håll älgen, Örjan!", "Håll älgen, Örjan!"),
        ("<p>hej!</p>", "&lt;p&gt;hej!&lt;/p&gt;"),
        ("bread & butter", "bread &amp; butter"),
        ("\"bread\" & butter", "&quot;bread&quot; &amp; butter"),
        ("< less than", "&lt; less than"),
        ("greater than >", "greater than &gt;"),
        ];

    for &(input, expected) in data.iter() {
        let actual = encode_minimal(input);
        assert_eq!(&actual, expected);
    }
}

#[test]
fn test_encode_attribute() {
    let data = [
        ("", ""),
        ("0 3px", "0&#x20;3px"),
        ("<img \"\"\">", "&lt;img&#x20;&quot;&quot;&quot;&gt;"),
        ("hej; hå", "hej&#x3B;&#x20;h&#xE5;"),
        ];
    for &(input, expected) in data.iter() {
        let actual = encode_attribute(input);
        assert_eq!(&actual, expected);
    }
}

#[test]
fn test_decode() {
    let data = [
        ("", ""),
        ("Håll älgen, Örjan!", "Håll älgen, Örjan!"),
        ("&lt;p&gt;hej!&lt;/p&gt;", "<p>hej!</p>"),
        ("hej&#x3B;&#x20;hå", "hej; hå"),
        ("&quot;width&#x3A;&#32;3px&#59;&quot;", "\"width: 3px;\""),
        ("&#x2b;", "+"),
        ];
    for &(input, expected) in data.iter() {
        match decode_html(input) {
            Ok(actual) => assert_eq!(&actual, expected),
            Err(reason) => panic!("Failed at \"{}\", reason \"{:?}\"", input, reason)
        }
    }
}

test_decode_err!(overflow_num, "&#100000000000000;", 0, MalformedNumEscape);
test_decode_err!(bad_unicode, "&#xffffffff;", 0, InvalidCharacter);
test_decode_err!(unterminated_named, "hej &amp", 4, PrematureEnd);
test_decode_err!(unterminated_dec, "hej &#", 4, PrematureEnd);
test_decode_err!(unterminated_hex, "hej &#x", 4, PrematureEnd);
test_decode_err!(dec_with_x, " &#01x1", 1, MalformedNumEscape);
test_decode_err!(unknown_entity, "  &hopp;", 2, UnknownEntity);
test_decode_err!(negative_dec, "   &#-1;", 3, MalformedNumEscape);
test_decode_err!(double_amp, "&&gt;;", 0, UnknownEntity);
test_decode_err!(empty_named, "&;", 0, UnknownEntity);
test_decode_err!(empty_dec, "&#;", 0, MalformedNumEscape);
test_decode_err!(empty_hex, "&#x;", 0, MalformedNumEscape);

#[test]
fn random_roundtrip() {
    let mut rng = rand::weak_rng();
    for _ in 1..100 {
        let original = random_str(&mut rng);
        let encoded = encode_attribute(&original);
        match decode_html(&encoded) {
            Err(reason) => panic!("error at \"{}\", reason: {:?}", original, reason),
            Ok(decoded) => assert_eq!(original, decoded)
        };
    }
}


fn random_str<R: rand::Rng>(rng: &mut R) -> String {
    let len = rng.gen_range::<usize>(0, 40);
    let mut s = String::new();
    for _ in 0..len {
        let c = char::from_u32(rng.gen_range::<usize>(1, 512) as u32).unwrap();
        s.push(c);
    }
    return s;
}

