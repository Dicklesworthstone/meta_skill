//! # Serde Regex
//!
//! [Documentation](https://docs.rs/serde_regex) |
//! [Github](https://github.com/tailhook/serde-regex) |
//! [Crate](https://crates.io/crates/serde_regex)
//!
//! A (de)serializer for `regex::Regex`
//!
//! # Example
//!
//! ```rust
//!
//! use regex::Regex;
//! use serde::{Deserialize, Serialize};
//! use serde_derive::{Serialize, Deserialize};
//!
//!
//! #[derive(Serialize, Deserialize)]
//! struct Timestamps {
//!     #[serde(with = "serde_regex")]
//!     pattern: Regex,
//! }
//!
//! #
//! # fn main() {}
//! ```
#![warn(missing_docs)]
#![warn(missing_debug_implementations)]

use regex::{Regex, RegexSet, bytes};
use std::fmt;
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use serde::de::{Error, Visitor};
use serde::{ser::SerializeSeq, Serialize, Serializer};
use serde::{Deserialize, Deserializer, de::SeqAccess};

/// A wrapper type which implements `Serialize` and `Deserialize` for
/// types involving `Regex`
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct Serde<T>(pub T);

struct RegexVecVisitor;
struct BytesRegexVecVisitor;

impl<'a> Visitor<'a> for RegexVecVisitor {
    type Value = Serde<Vec<Regex>>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("valid sequence")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'a>,
    {
        let mut vec = match seq.size_hint() {
            Some(size) => Vec::with_capacity(size),
            None => Vec::new(),
        };
        while let Some(Serde(el)) = seq.next_element()? {
            vec.push(el);
        }
        return Ok(Serde(vec));
    }
}

impl<'a> Visitor<'a> for BytesRegexVecVisitor {
    type Value = Serde<Vec<bytes::Regex>>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("valid sequence")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'a>,
    {
        let mut vec = match seq.size_hint() {
            Some(size) => Vec::with_capacity(size),
            None => Vec::new(),
        };
        while let Some(Serde(el)) = seq.next_element()? {
            vec.push(el);
        }
        return Ok(Serde(vec));
    }
}

impl<'de> Deserialize<'de> for Serde<Option<Regex>> {
    fn deserialize<D>(d: D) -> Result<Serde<Option<Regex>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<Serde<Regex>>::deserialize(d)? {
            Some(Serde(regex)) => Ok(Serde(Some(regex))),
            None => Ok(Serde(None)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<Regex> {
    fn deserialize<D>(d: D) -> Result<Serde<Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Cow<str>>::deserialize(d)?;

        match s.parse() {
            Ok(regex) => Ok(Serde(regex)),
            Err(err) => Err(D::Error::custom(err)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<RegexSet> {
    fn deserialize<D>(d: D) -> Result<Serde<RegexSet>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regexes = <Vec<Cow<str>>>::deserialize(d)?;

        match RegexSet::new(regexes) {
            Ok(regexset) => Ok(Serde(regexset)),
            Err(err) => Err(D::Error::custom(err)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<Vec<Regex>> {
    fn deserialize<D>(d: D) -> Result<Serde<Vec<Regex>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_seq(RegexVecVisitor)
    }
}

impl<'de> Deserialize<'de> for Serde<Option<Vec<bytes::Regex>>> {
    fn deserialize<D>(d: D) -> Result<Serde<Option<Vec<bytes::Regex>>>, D::Error>
    where
        D: Deserializer<'de>,
    {
         match Option::<Serde<Vec<bytes::Regex>>>::deserialize(d)? {
            Some(Serde(regex)) => Ok(Serde(Some(regex))),
            None => Ok(Serde(None)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<Option<Vec<Regex>>> {
    fn deserialize<D>(d: D) -> Result<Serde<Option<Vec<Regex>>>, D::Error>
    where
        D: Deserializer<'de>,
    {
         match Option::<Serde<Vec<Regex>>>::deserialize(d)? {
            Some(Serde(regex)) => Ok(Serde(Some(regex))),
            None => Ok(Serde(None)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<Option<bytes::Regex>> {
    fn deserialize<D>(d: D) -> Result<Serde<Option<bytes::Regex>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<Serde<bytes::Regex>>::deserialize(d)? {
            Some(Serde(regex)) => Ok(Serde(Some(regex))),
            None => Ok(Serde(None)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<bytes::Regex> {
    fn deserialize<D>(d: D) -> Result<Serde<bytes::Regex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Cow<str>>::deserialize(d)?;

        match s.parse() {
            Ok(regex) => Ok(Serde(regex)),
            Err(err) => Err(D::Error::custom(err)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<bytes::RegexSet> {
    fn deserialize<D>(d: D) -> Result<Serde<bytes::RegexSet>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let regexes = <Vec<Cow<str>>>::deserialize(d)?;

        match bytes::RegexSet::new(regexes) {
            Ok(regexset) => Ok(Serde(regexset)),
            Err(err) => Err(D::Error::custom(err)),
        }
    }
}

impl<'de> Deserialize<'de> for Serde<Vec<bytes::Regex>> {
    fn deserialize<D>(d: D) -> Result<Serde<Vec<bytes::Regex>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        d.deserialize_seq(BytesRegexVecVisitor)
    }
}
/// Deserialize function, see crate docs to see how to use it
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    Serde<T>: Deserialize<'de>,
{
    Serde::deserialize(deserializer).map(|x| x.0)
}

/// Deserialize function, see crate docs to see how to use it
pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    for<'a> Serde<&'a T>: Serialize,
{
    Serde(value).serialize(serializer)
}

impl<T> Deref for Serde<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Serde<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Serde<T> {
    /// Consumes the `Serde`, returning the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Serde<T> {
    fn from(val: T) -> Serde<T> {
        Serde(val)
    }
}

impl<'a> Serialize for Serde<&'a Regex> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.as_str().serialize(serializer)
    }
}

impl Serialize for Serde<Regex> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.as_str().serialize(serializer)
    }
}

impl Serialize for Serde<RegexSet> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.patterns().serialize(serializer)
    }
}

impl<'a> Serialize for Serde<&'a Option<Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            &Some(ref value) => serializer.serialize_some(&Serde(value)),
            &None => serializer.serialize_none(),
        }
    }
}

impl Serialize for Serde<Option<Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serde(&self.0).serialize(serializer)
    }
}

impl Serialize for Serde<Vec<Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serde(&self.0).serialize(serializer)
    }
}

impl<'a> Serialize for Serde<&'a Vec<Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for element in self.0 {
            seq.serialize_element(&Serde(element))?;
        }
        seq.end()
    }
}

impl<'a> Serialize for Serde<&'a bytes::Regex> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.as_str().serialize(serializer)
    }
}

impl Serialize for Serde<bytes::Regex> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.as_str().serialize(serializer)
    }
}

impl<'a> Serialize for Serde<&'a Option<bytes::Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            &Some(ref value) => serializer.serialize_some(&Serde(value)),
            &None => serializer.serialize_none(),
        }
    }
}

impl Serialize for Serde<Option<bytes::Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serde(&self.0).serialize(serializer)
    }
}

impl Serialize for Serde<bytes::RegexSet> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.patterns().serialize(serializer)
    }
}

impl Serialize for Serde<Vec<bytes::Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Serde(&self.0).serialize(serializer)
    }
}

impl<'a> Serialize for Serde<&'a Vec<bytes::Regex>> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for element in self.0 {
            seq.serialize_element(&Serde(element))?;
        }
        seq.end()
    }
}

#[cfg(test)]
mod test {
    use serde_json::{json, from_value, from_str, to_string, to_value};
    use regex::{Regex, RegexSet, bytes};
    use crate::Serde;

    const SAMPLE: &str = r#"[a-z"\]]+\d{1,10}""#;
    const SAMPLE_JSON: &str = r#""[a-z\"\\]]+\\d{1,10}\"""#;

    #[test]
    fn test_set() -> Result<(), Box<dyn std::error::Error>> {
        let regexes = &[
            "my(regex)?",
            "other[regex]+",
        ];
        let json = json!(regexes);
        let set: Serde<RegexSet> = from_value(json.clone())?;
        assert_eq!(set.patterns(), regexes);
        assert_eq!(
            to_value(set).expect("serialization error"),
            json
        );
        Ok(())
    }

    #[test]
    fn test_vec() -> Result<(), Box<dyn std::error::Error>> {
        let json = json!(["a.*b", "c?d"]);
        let vec: Serde<Vec<Regex>> = from_value(json)?;
        assert!(vec.0[0].as_str() == "a.*b");
        assert!(vec.0[1].as_str() == "c?d");
        assert!(vec.len() == 2);
        Ok(())
    }

    #[test]
    fn test_simple() {
        let re: Serde<Regex> = from_str(SAMPLE_JSON).unwrap();
        assert_eq!(re.as_str(), SAMPLE);
        assert_eq!(to_string(&re).unwrap(), SAMPLE_JSON);
    }

    #[test]
    fn test_option_some() {
        let re: Serde<Option<Regex>> = from_str(SAMPLE_JSON).unwrap();
        assert_eq!(re.as_ref().map(|regex| regex.as_str()), Some(SAMPLE));
        assert_eq!(to_string(&re).unwrap(), SAMPLE_JSON);
    }

    #[test]
    fn test_option_none() {
        let re: Serde<Option<Regex>> = from_str("null").unwrap();
        assert!(re.is_none());
        assert_eq!(to_string(&re).unwrap(), "null");
    }

    #[test]
    fn test_set_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let regexes = &[
            "regex.*test",
            "test( )??regex+",
        ];
        let json = json!(regexes);
        let set: Serde<bytes::RegexSet> = from_value(json.clone())?;
        assert_eq!(set.patterns(), regexes);
        assert_eq!(
            to_value(set).expect("serialization error"),
            json
        );
        Ok(())
    }

    #[test]
    fn test_vec_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let json = json!(["a.*b", "c?d"]);
        let vec: Serde<Vec<bytes::Regex>> = from_value(json)?;
        assert!(vec.0[0].as_str() == "a.*b");
        assert!(vec.0[1].as_str() == "c?d");
        assert!(vec.len() == 2);
        Ok(())
    }

    #[test]
    fn test_option_vec() -> Result<(), Box<dyn std::error::Error>> {
        let json = json!(["a.*b", "c?d"]);
        let vec: Serde<Option<Vec<Regex>>> = from_value(json)?;
        assert!(vec.is_some());
        let v = vec.0.unwrap();
        assert!(v[0].as_str() == "a.*b");
        assert!(v[1].as_str() == "c?d");
        assert!(v.len() == 2);
        Ok(())
    }
    #[test]
    fn test_option_vec_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let json = json!(["a.*b", "c?d"]);
        let vec: Serde<Option<Vec<bytes::Regex>>> = from_value(json)?;
        assert!(vec.is_some());
        let v = vec.0.unwrap();
        assert!(v[0].as_str() == "a.*b");
        assert!(v[1].as_str() == "c?d");
        assert!(v.len() == 2);
        Ok(())
    }
    #[test]
    fn test_option_vec_none() -> Result<(), Box<dyn std::error::Error>> {
        let vec: Serde<Option<Vec<bytes::Regex>>> = from_str("null")?;
        assert!(vec.is_none());
        Ok(())
    }

    #[test]
    fn test_bytes_simple() {
        let re: Serde<bytes::Regex> = from_str(SAMPLE_JSON).unwrap();
        assert_eq!(re.as_str(), SAMPLE);
        assert_eq!(to_string(&re).unwrap(), SAMPLE_JSON);
    }

    #[test]
    fn test_bytes_option_some() {
        let re: Serde<Option<bytes::Regex>> = from_str(SAMPLE_JSON).unwrap();
        assert_eq!(re.as_ref().map(|regex| regex.as_str()), Some(SAMPLE));
        assert_eq!(to_string(&re).unwrap(), SAMPLE_JSON);
    }

    #[test]
    fn test_bytes_option_none() {
        let re: Serde<Option<bytes::Regex>> = from_str("null").unwrap();
        assert!(re.is_none());
        assert_eq!(to_string(&re).unwrap(), "null");
    }
}
