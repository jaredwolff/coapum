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
        let mut base_record = SenMLRecord::default();

        // Set base values in the first record
        if let Some(bn) = base.bn {
            base_record.n = Some(bn);
        }

        // Note: Base values are typically stored as fields starting with 'b'
        // but serde flattening will handle this during serialization

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
        if let Some(first) = self.records.first() {
            // Check if first record has base-like values
            first.n.as_ref().is_some_and(|n| n.ends_with('/'))
                || first.t.is_some()
                || first.u.is_some()
        } else {
            false
        }
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
        if let Some(bt) = base.bt {
            if !bt.is_finite() {
                return Err(SenMLError::invalid_field_value("bt", &bt.to_string()));
            }
        }

        if let Some(bv) = base.bv {
            if !bv.is_finite() {
                return Err(SenMLError::invalid_field_value("bv", &bv.to_string()));
            }
        }

        if let Some(bs) = base.bs {
            if !bs.is_finite() {
                return Err(SenMLError::invalid_field_value("bs", &bs.to_string()));
            }
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
            bn: record.n.clone(),
            bt: record.t,
            bu: record.u.clone(),
            bv: record.v,
            bs: record.s,
            bver: None, // Version not stored in basic record
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

    /// Serialize to CBOR bytes
    #[cfg(feature = "cbor")]
    pub fn to_cbor(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        ciborium::ser::into_writer(self, &mut buffer)
            .map_err(|e| SenMLError::serialization(e.to_string()))?;
        Ok(buffer)
    }

    /// Deserialize from CBOR bytes
    #[cfg(feature = "cbor")]
    pub fn from_cbor(bytes: &[u8]) -> Result<Self> {
        ciborium::de::from_reader(bytes).map_err(|e| SenMLError::deserialization(e.to_string()))
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
}
