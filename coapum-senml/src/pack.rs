//! SenML Pack - collection of SenML records

use crate::{Result, SenMLError, SenMLRecord};
use serde::{Deserialize, Serialize};

#[cfg(feature = "validation")]
use validator::Validate;

/// A SenML Pack represents a collection of SenML records with optional base values
///
/// According to RFC 8428, a SenML Pack is an array of SenML Records. The first
/// record can contain base values (fields starting with 'b') that apply to
/// subsequent records, reducing redundancy in the representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SenMLPack {
    /// Array of SenML records
    pub records: Vec<SenMLRecord>,
}

/// Base values that can be applied to multiple records in a pack
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "validation", derive(Validate))]
pub struct BaseValues {
    /// Base Name - prepended to record names
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bn: Option<String>,

    /// Base Time - added to record timestamps  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bt: Option<f64>,

    /// Base Unit - used when record has no unit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bu: Option<String>,

    /// Base Value - added to numeric record values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bv: Option<f64>,

    /// Base Sum - added to sum values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bs: Option<f64>,

    /// Base Version - SenML version number  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bver: Option<i32>,
}

impl SenMLPack {
    /// Create a new empty pack
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Create a pack with base values
    pub fn with_base_values(base: BaseValues) -> Self {
        let base_record = SenMLRecord {
            bn: base.bn,
            bt: base.bt,
            bu: base.bu,
            bv: base.bv,
            bs: base.bs,
            bver: base.bver,
            ..Default::default()
        };

        Self {
            records: vec![base_record],
        }
    }

    /// Add a record to this pack
    pub fn add_record(&mut self, record: SenMLRecord) {
        self.records.push(record);
    }

    /// Add multiple records to this pack
    pub fn add_records<I>(&mut self, records: I)
    where
        I: IntoIterator<Item = SenMLRecord>,
    {
        self.records.extend(records);
    }

    /// Get base values from the first record (if any)
    pub fn base_values(&self) -> BaseValues {
        self.records
            .first()
            .map(|record| self.extract_base_values(record))
            .unwrap_or_default()
    }

    /// Check if this pack has base values
    pub fn has_base_values(&self) -> bool {
        self.records
            .first()
            .is_some_and(|first| first.has_base_fields())
    }

    /// Get the number of records in this pack
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Check if this pack is empty
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterate over records in this pack
    pub fn iter(&self) -> impl Iterator<Item = &SenMLRecord> {
        self.records.iter()
    }

    /// Get a mutable iterator over records
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut SenMLRecord> {
        self.records.iter_mut()
    }

    /// Validate this pack according to RFC 8428
    pub fn validate(&self) -> Result<()> {
        if self.records.is_empty() {
            return Err(SenMLError::validation("SenML pack cannot be empty"));
        }

        // Validate each record
        for (i, record) in self.records.iter().enumerate() {
            record.validate().map_err(|e| {
                SenMLError::validation(format!("Invalid record at index {}: {}", i, e))
            })?;
        }

        // Validate base values if present
        let base = self.base_values();
        if let Some(bt) = base.bt
            && !bt.is_finite()
        {
            return Err(SenMLError::invalid_field_value("bt", &bt.to_string()));
        }

        if let Some(bv) = base.bv
            && !bv.is_finite()
        {
            return Err(SenMLError::invalid_field_value("bv", &bv.to_string()));
        }

        if let Some(bs) = base.bs
            && !bs.is_finite()
        {
            return Err(SenMLError::invalid_field_value("bs", &bs.to_string()));
        }

        Ok(())
    }

    /// Convert this pack to a normalized form
    pub fn normalize(&self) -> crate::normalize::NormalizedPack {
        crate::normalize::NormalizedPack::from_pack(self)
    }

    /// Extract base values from a record (typically the first one)
    fn extract_base_values(&self, record: &SenMLRecord) -> BaseValues {
        BaseValues {
            bn: record.bn.clone(),
            bt: record.bt,
            bu: record.bu.clone(),
            bv: record.bv,
            bs: record.bs,
            bver: record.bver,
        }
    }
}

impl Default for SenMLPack {
    fn default() -> Self {
        Self::new()
    }
}

impl FromIterator<SenMLRecord> for SenMLPack {
    fn from_iter<I: IntoIterator<Item = SenMLRecord>>(iter: I) -> Self {
        Self {
            records: iter.into_iter().collect(),
        }
    }
}

impl IntoIterator for SenMLPack {
    type Item = SenMLRecord;
    type IntoIter = std::vec::IntoIter<SenMLRecord>;

    fn into_iter(self) -> Self::IntoIter {
        self.records.into_iter()
    }
}

impl<'a> IntoIterator for &'a SenMLPack {
    type Item = &'a SenMLRecord;
    type IntoIter = std::slice::Iter<'a, SenMLRecord>;

    fn into_iter(self) -> Self::IntoIter {
        self.records.iter()
    }
}

// Convenience methods for serialization
impl SenMLPack {
    /// Serialize to JSON string
    #[cfg(feature = "json")]
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| SenMLError::serialization(e.to_string()))
    }

    /// Serialize to pretty JSON string
    #[cfg(feature = "json")]
    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| SenMLError::serialization(e.to_string()))
    }

    /// Deserialize from JSON string
    #[cfg(feature = "json")]
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| SenMLError::deserialization(e.to_string()))
    }

    /// Serialize to CBOR bytes using RFC 8428 integer labels (Table 6).
    #[cfg(feature = "cbor")]
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        use ciborium::Value;

        let array: Vec<Value> = self.records.iter().map(record_to_cbor_value).collect();
        let mut buffer = Vec::new();
        ciborium::ser::into_writer(&Value::Array(array), &mut buffer)
            .map_err(|e| SenMLError::serialization(e.to_string()))?;
        Ok(buffer)
    }

    /// Deserialize from CBOR bytes using RFC 8428 integer labels (Table 6).
    ///
    /// Uses a recursion depth limit of 32 to prevent stack overflow from
    /// maliciously crafted deeply-nested CBOR payloads.
    #[cfg(feature = "cbor")]
    pub fn from_cbor(bytes: &[u8]) -> Result<Self> {
        use ciborium::Value;
        const MAX_CBOR_RECURSION_DEPTH: usize = 32;

        let value: Value =
            ciborium::de::from_reader_with_recursion_limit(bytes, MAX_CBOR_RECURSION_DEPTH)
                .map_err(|e| SenMLError::deserialization(e.to_string()))?;

        let array = match value {
            Value::Array(a) => a,
            _ => return Err(SenMLError::deserialization("expected CBOR array")),
        };

        let records = array
            .into_iter()
            .map(cbor_value_to_record)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { records })
    }
}

/// RFC 8428 Table 6: CBOR integer labels for SenML fields.
#[cfg(feature = "cbor")]
mod cbor_labels {
    pub const BN: i64 = -2;
    pub const BT: i64 = -3;
    pub const BU: i64 = -4;
    pub const BV: i64 = -5;
    pub const BS: i64 = -6;
    pub const BVER: i64 = -1;
    pub const N: i64 = 0;
    pub const U: i64 = 1;
    pub const V: i64 = 2;
    pub const VS: i64 = 3;
    pub const VB: i64 = 4;
    pub const VD: i64 = 8;
    pub const S: i64 = 5;
    pub const T: i64 = 6;
    pub const UT: i64 = 7;
}

/// Convert a SenMLRecord to a CBOR Value map with integer keys.
#[cfg(feature = "cbor")]
fn record_to_cbor_value(record: &SenMLRecord) -> ciborium::Value {
    use cbor_labels::*;
    use ciborium::Value;

    let mut pairs = Vec::new();
    macro_rules! push_opt {
        ($label:expr, $field:expr, $conv:expr) => {
            if let Some(ref val) = $field {
                pairs.push((Value::Integer($label.into()), $conv(val)));
            }
        };
    }
    push_opt!(BN, record.bn, |v: &String| Value::Text(v.clone()));
    push_opt!(BT, record.bt, |v: &f64| Value::Float(*v));
    push_opt!(BU, record.bu, |v: &String| Value::Text(v.clone()));
    push_opt!(BV, record.bv, |v: &f64| Value::Float(*v));
    push_opt!(BS, record.bs, |v: &f64| Value::Float(*v));
    push_opt!(BVER, record.bver, |v: &i32| Value::Integer(
        (*v as i64).into()
    ));
    push_opt!(N, record.n, |v: &String| Value::Text(v.clone()));
    push_opt!(U, record.u, |v: &String| Value::Text(v.clone()));
    push_opt!(V, record.v, |v: &f64| Value::Float(*v));
    push_opt!(VS, record.vs, |v: &String| Value::Text(v.clone()));
    push_opt!(VB, record.vb, |v: &bool| Value::Bool(*v));
    push_opt!(VD, record.vd, |v: &String| Value::Text(v.clone()));
    push_opt!(S, record.s, |v: &f64| Value::Float(*v));
    push_opt!(T, record.t, |v: &f64| Value::Float(*v));
    push_opt!(UT, record.ut, |v: &f64| Value::Float(*v));

    Value::Map(pairs)
}

/// Convert a CBOR Value map with integer keys to a SenMLRecord.
#[cfg(feature = "cbor")]
fn cbor_value_to_record(value: ciborium::Value) -> Result<SenMLRecord> {
    use cbor_labels::*;
    use ciborium::Value;

    let pairs = match value {
        Value::Map(pairs) => pairs,
        _ => return Err(SenMLError::deserialization("expected CBOR map for record")),
    };

    let mut record = SenMLRecord::default();

    for (key, val) in pairs {
        let label: i64 = match key {
            Value::Integer(i) => {
                let v = i128::from(i);
                if v >= i64::MIN as i128 && v <= i64::MAX as i128 {
                    v as i64
                } else {
                    continue;
                }
            }
            _ => continue, // skip non-integer keys
        };

        match label {
            BN => record.bn = val.into_text().ok(),
            BT => record.bt = as_f64(&val),
            BU => record.bu = val.into_text().ok(),
            BV => record.bv = as_f64(&val),
            BS => record.bs = as_f64(&val),
            BVER => record.bver = as_i32(&val),
            N => record.n = val.into_text().ok(),
            U => record.u = val.into_text().ok(),
            V => record.v = as_f64(&val),
            VS => record.vs = val.into_text().ok(),
            VB => {
                if let Value::Bool(b) = val {
                    record.vb = Some(b);
                }
            }
            VD => record.vd = val.into_text().ok(),
            S => record.s = as_f64(&val),
            T => record.t = as_f64(&val),
            UT => record.ut = as_f64(&val),
            _ => {} // unknown label — ignore
        }
    }

    Ok(record)
}

#[cfg(feature = "cbor")]
fn as_f64(val: &ciborium::Value) -> Option<f64> {
    match val {
        ciborium::Value::Float(f) => Some(*f),
        ciborium::Value::Integer(i) => Some(i128::from(*i) as f64),
        _ => None,
    }
}

#[cfg(feature = "cbor")]
fn as_i32(val: &ciborium::Value) -> Option<i32> {
    match val {
        ciborium::Value::Integer(i) => {
            let v = i128::from(*i);
            if v >= i32::MIN as i128 && v <= i32::MAX as i128 {
                Some(v as i32)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SenMLRecord;

    #[test]
    fn test_empty_pack_creation() {
        let pack = SenMLPack::new();
        assert!(pack.is_empty());
        assert_eq!(pack.len(), 0);
    }

    #[test]
    fn test_pack_with_records() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temperature", 22.5));
        pack.add_record(SenMLRecord::with_value("humidity", 45.0));

        assert_eq!(pack.len(), 2);
        assert!(!pack.is_empty());
    }

    #[test]
    fn test_pack_iteration() {
        let records = vec![
            SenMLRecord::with_value("temp", 20.0),
            SenMLRecord::with_value("humidity", 50.0),
        ];
        let pack: SenMLPack = records.into_iter().collect();

        let mut count = 0;
        for record in &pack {
            count += 1;
            assert!(record.has_value());
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_pack_validation() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 25.0));

        assert!(pack.validate().is_ok());

        let empty_pack = SenMLPack::new();
        assert!(empty_pack.validate().is_err());
    }

    #[cfg(feature = "json")]
    #[test]
    fn test_json_serialization() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temperature", 22.5));

        let json = pack.to_json().unwrap();
        let deserialized = SenMLPack::from_json(&json).unwrap();

        assert_eq!(pack, deserialized);
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn test_cbor_serialization() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temperature", 22.5));

        let cbor = pack.to_cbor().unwrap();
        let deserialized = SenMLPack::from_cbor(&cbor).unwrap();

        assert_eq!(pack, deserialized);
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn test_cbor_integer_keys_on_wire() {
        use ciborium::Value;

        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord {
            bn: Some("device/".to_string()),
            n: Some("temp".to_string()),
            v: Some(25.0),
            ..Default::default()
        });

        let cbor = pack.to_cbor().unwrap();

        // Decode raw CBOR to inspect keys
        let raw: Value = ciborium::de::from_reader(&cbor[..]).unwrap();
        let array = raw.as_array().unwrap();
        let map = array[0].as_map().unwrap();

        // Verify integer keys: bn=-2, n=0, v=2
        let keys: Vec<i128> = map
            .iter()
            .map(|(k, _)| i128::from(k.as_integer().unwrap()))
            .collect();
        assert!(keys.contains(&-2), "missing bn key (-2)");
        assert!(keys.contains(&0), "missing n key (0)");
        assert!(keys.contains(&2), "missing v key (2)");
        // No string keys should be present
        assert!(
            map.iter().all(|(k, _)| k.as_integer().is_some()),
            "all keys should be integers"
        );
    }

    #[cfg(feature = "cbor")]
    #[test]
    fn test_cbor_roundtrip_with_base_fields() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord {
            bn: Some("sensor/".to_string()),
            bt: Some(1640995200.0),
            bu: Some("Cel".to_string()),
            bv: Some(20.0),
            bs: Some(100.0),
            bver: Some(10),
            n: Some("temp".to_string()),
            v: Some(2.5),
            t: Some(60.0),
            ..Default::default()
        });

        let cbor = pack.to_cbor().unwrap();
        let restored = SenMLPack::from_cbor(&cbor).unwrap();
        assert_eq!(pack, restored);
    }
}
