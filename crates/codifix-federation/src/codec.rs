// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Serialize;
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

use crate::{ContentDigest, FederationError};

#[derive(Debug, Default, Clone, Copy)]
pub struct CanonicalJsonCodec;

impl CanonicalJsonCodec {
    pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FederationError> {
        validate_finite_numbers(value)?;
        serde_json_canonicalizer::to_vec(value).map_err(|error| FederationError::CanonicalJson {
            message: error.to_string(),
        })
    }

    pub fn encode_string<T: Serialize>(value: &T) -> Result<String, FederationError> {
        let encoded = Self::encode(value)?;
        String::from_utf8(encoded).map_err(|error| FederationError::CanonicalJson {
            message: error.to_string(),
        })
    }

    pub fn digest<T: Serialize>(value: &T) -> Result<ContentDigest, FederationError> {
        let encoded = Self::encode(value)?;
        Ok(ContentDigest::sha256_bytes(&encoded))
    }
}

fn validate_finite_numbers<T: Serialize>(value: &T) -> Result<(), FederationError> {
    value
        .serialize(FiniteNumberValidator)
        .map_err(|error| FederationError::CanonicalJson {
            message: error.to_string(),
        })
}

struct FiniteNumberValidator;

struct CompoundValidator;

#[derive(Debug)]
struct ValidationSerializerError(String);

impl std::fmt::Display for ValidationSerializerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

impl std::error::Error for ValidationSerializerError {}

impl ser::Error for ValidationSerializerError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self(msg.to_string())
    }
}

impl Serializer for FiniteNumberValidator {
    type Ok = ();
    type Error = ValidationSerializerError;
    type SerializeSeq = CompoundValidator;
    type SerializeTuple = CompoundValidator;
    type SerializeTupleStruct = CompoundValidator;
    type SerializeTupleVariant = CompoundValidator;
    type SerializeMap = CompoundValidator;
    type SerializeStruct = CompoundValidator;
    type SerializeStructVariant = CompoundValidator;

    fn serialize_bool(self, _: bool) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i8(self, _: i8) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i16(self, _: i16) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i32(self, _: i32) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i64(self, _: i64) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i128(self, _: i128) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u8(self, _: u8) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u16(self, _: u16) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u32(self, _: u32) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u64(self, _: u64) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u128(self, _: u128) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        if value.is_finite() {
            return Ok(());
        }
        Err(ValidationSerializerError(
            "non-finite f32 values are not valid canonical JSON".to_string(),
        ))
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if value.is_finite() {
            return Ok(());
        }
        Err(ValidationSerializerError(
            "non-finite f64 values are not valid canonical JSON".to_string(),
        ))
    }

    fn serialize_char(self, _: char) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_str(self, _: &str) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_newtype_struct<T>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(CompoundValidator)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(CompoundValidator)
    }

    fn collect_str<T>(self, _: &T) -> Result<Self::Ok, Self::Error>
    where
        T: std::fmt::Display + ?Sized,
    {
        Ok(())
    }
}

impl SerializeSeq for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTuple for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleStruct for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleVariant for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeMap for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        key.serialize(FiniteNumberValidator)
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStruct for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_field<T>(&mut self, _: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStructVariant for CompoundValidator {
    type Ok = ();
    type Error = ValidationSerializerError;

    fn serialize_field<T>(&mut self, _: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize + ?Sized,
    {
        value.serialize(FiniteNumberValidator)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde::Serialize;

    use super::*;

    #[derive(Debug, Serialize)]
    struct FloatWrapper {
        value: f64,
    }

    #[test]
    fn rejects_non_finite_numbers() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let error = CanonicalJsonCodec::encode(&FloatWrapper { value })
                .expect_err("non-finite float should fail JCS encoding");
            match error {
                FederationError::CanonicalJson { .. } => {}
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    proptest! {
        #[test]
        fn digest_is_deterministic_for_repeated_encoding(
            pairs in prop::collection::vec(("[a-z]{1,8}", "[a-zA-Z0-9_-]{0,12}"), 1..8)
        ) {
            let value = std::collections::BTreeMap::<String, String>::from_iter(pairs);
            let first = CanonicalJsonCodec::digest(&value).expect("digest should succeed");
            let second = CanonicalJsonCodec::digest(&value).expect("digest should succeed");
            prop_assert_eq!(first, second);
        }
    }
}
