//! SenML Record types and values

use serde::{Deserialize, Serialize};

#[cfg(feature = "validation")]
use validator::Validate;

/// A SenML Record represents a single sensor measurement or device parameter
///
/// According to RFC 8428, a record contains optional fields for identifying
/// the measurement (name), its value, unit, timestamp, and other metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "validation", derive(Validate))]
#[derive(Default)]
pub struct SenMLRecord {
    /// Name - identifies the sensor or parameter  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,

    /// Unit - SI unit or custom unit string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub u: Option<String>,

    /// Value - numeric measurement value
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "validation", validate(range(min = -1e38, max = 1e38)))]
    pub v: Option<f64>,

    /// String Value - textual measurement value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vs: Option<String>,

    /// Boolean Value - true/false measurement value  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vb: Option<bool>,

    /// Data Value - base64-encoded binary data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vd: Option<String>,

    /// Sum - integrated sum of values over time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<f64>,

    /// Time - timestamp relative to base time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<f64>,

    /// Update Time - maximum time before next update
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ut: Option<f64>,
}

/// Union type for SenML values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SenMLValue {
    /// Numeric value
    Number(f64),
    /// String value
    String(String),
    /// Boolean value
    Boolean(bool),
    /// Binary data (base64 encoded)
    Data(Vec<u8>),
}

impl SenMLRecord {
    /// Create a new empty record
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a record with a numeric value
    pub fn with_value<S: Into<String>>(name: S, value: f64) -> Self {
        Self {
            n: Some(name.into()),
            v: Some(value),
            ..Default::default()
        }
    }

    /// Create a record with a string value
    pub fn with_string_value<S: Into<String>, V: Into<String>>(name: S, value: V) -> Self {
        Self {
            n: Some(name.into()),
            vs: Some(value.into()),
            ..Default::default()
        }
    }

    /// Create a record with a boolean value
    pub fn with_bool_value<S: Into<String>>(name: S, value: bool) -> Self {
        Self {
            n: Some(name.into()),
            vb: Some(value),
            ..Default::default()
        }
    }

    /// Create a record with binary data
    pub fn with_data_value<S: Into<String>>(name: S, data: Vec<u8>) -> Self {
        let encoded = base64_encode(&data);
        Self {
            n: Some(name.into()),
            vd: Some(encoded),
            ..Default::default()
        }
    }

    /// Set the unit for this record
    pub fn with_unit<S: Into<String>>(mut self, unit: S) -> Self {
        self.u = Some(unit.into());
        self
    }

    /// Set the timestamp for this record
    pub fn with_time(mut self, time: f64) -> Self {
        self.t = Some(time);
        self
    }

    /// Set the sum value for this record
    pub fn with_sum(mut self, sum: f64) -> Self {
        self.s = Some(sum);
        self
    }

    /// Get the primary value from this record
    pub fn value(&self) -> Option<SenMLValue> {
        if let Some(v) = self.v {
            Some(SenMLValue::Number(v))
        } else if let Some(ref vs) = self.vs {
            Some(SenMLValue::String(vs.clone()))
        } else if let Some(vb) = self.vb {
            Some(SenMLValue::Boolean(vb))
        } else if let Some(ref vd) = self.vd {
            if let Ok(data) = base64_decode(vd) {
                Some(SenMLValue::Data(data))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if this record has a value
    pub fn has_value(&self) -> bool {
        self.v.is_some() || self.vs.is_some() || self.vb.is_some() || self.vd.is_some()
    }

    /// Get the name of this record, resolving with base name if needed
    pub fn resolved_name(&self, base_name: Option<&str>) -> Option<String> {
        match (&self.n, base_name) {
            (Some(name), Some(base)) => Some(format!("{}{}", base, name)),
            (Some(name), None) => Some(name.clone()),
            (None, Some(base)) => Some(base.to_string()),
            (None, None) => None,
        }
    }

    /// Validate this record according to RFC 8428 rules
    pub fn validate(&self) -> crate::Result<()> {
        // A record must have at least one value field
        if !self.has_value() && self.s.is_none() {
            return Err(crate::SenMLError::validation(
                "Record must have at least one value field (v, vs, vb, vd, or s)",
            ));
        }

        // Validate time values
        if let Some(t) = self.t {
            if !t.is_finite() {
                return Err(crate::SenMLError::invalid_field_value("t", &t.to_string()));
            }
        }

        if let Some(ut) = self.ut {
            if !ut.is_finite() || ut < 0.0 {
                return Err(crate::SenMLError::invalid_field_value(
                    "ut",
                    &ut.to_string(),
                ));
            }
        }

        // Validate numeric values
        if let Some(v) = self.v {
            if !v.is_finite() {
                return Err(crate::SenMLError::invalid_field_value("v", &v.to_string()));
            }
        }

        if let Some(s) = self.s {
            if !s.is_finite() {
                return Err(crate::SenMLError::invalid_field_value("s", &s.to_string()));
            }
        }

        // Validate data field is valid base64
        if let Some(ref vd) = self.vd {
            if base64_decode(vd).is_err() {
                return Err(crate::SenMLError::invalid_field_value(
                    "vd",
                    "invalid base64",
                ));
            }
        }

        Ok(())
    }
}

impl From<SenMLValue> for SenMLRecord {
    fn from(value: SenMLValue) -> Self {
        match value {
            SenMLValue::Number(n) => Self {
                v: Some(n),
                ..Default::default()
            },
            SenMLValue::String(s) => Self {
                vs: Some(s),
                ..Default::default()
            },
            SenMLValue::Boolean(b) => Self {
                vb: Some(b),
                ..Default::default()
            },
            SenMLValue::Data(d) => Self {
                vd: Some(base64_encode(&d)),
                ..Default::default()
            },
        }
    }
}

// Helper functions for base64 encoding/decoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let chunks = data.chunks_exact(3);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let b1 = chunk[0] as u32;
        let b2 = chunk[1] as u32;
        let b3 = chunk[2] as u32;
        let combined = (b1 << 16) | (b2 << 8) | b3;

        result.push(ALPHABET[((combined >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((combined >> 12) & 0x3F) as usize] as char);
        result.push(ALPHABET[((combined >> 6) & 0x3F) as usize] as char);
        result.push(ALPHABET[(combined & 0x3F) as usize] as char);
    }

    match remainder.len() {
        1 => {
            let b1 = remainder[0] as u32;
            let combined = b1 << 16;
            result.push(ALPHABET[((combined >> 18) & 0x3F) as usize] as char);
            result.push(ALPHABET[((combined >> 12) & 0x3F) as usize] as char);
            result.push_str("==");
        }
        2 => {
            let b1 = remainder[0] as u32;
            let b2 = remainder[1] as u32;
            let combined = (b1 << 16) | (b2 << 8);
            result.push(ALPHABET[((combined >> 18) & 0x3F) as usize] as char);
            result.push(ALPHABET[((combined >> 12) & 0x3F) as usize] as char);
            result.push(ALPHABET[((combined >> 6) & 0x3F) as usize] as char);
            result.push('=');
        }
        _ => {}
    }

    result
}

fn base64_decode(s: &str) -> Result<Vec<u8>, &'static str> {
    // Simple base64 decoder - in production you'd use a proper library
    let chars: Vec<char> = s.chars().filter(|&c| c != '=').collect();
    let mut result = Vec::new();

    for chunk in chars.chunks(4) {
        if chunk.len() < 2 {
            return Err("Invalid base64");
        }

        let mut combined = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let val = match c {
                'A'..='Z' => (c as u32) - ('A' as u32),
                'a'..='z' => (c as u32) - ('a' as u32) + 26,
                '0'..='9' => (c as u32) - ('0' as u32) + 52,
                '+' => 62,
                '/' => 63,
                _ => return Err("Invalid base64 character"),
            };
            combined |= val << (6 * (3 - i));
        }

        result.push((combined >> 16) as u8);
        if chunk.len() > 2 {
            result.push((combined >> 8) as u8);
        }
        if chunk.len() > 3 {
            result.push(combined as u8);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_creation() {
        let record = SenMLRecord::with_value("temperature", 22.5);
        assert_eq!(record.n, Some("temperature".to_string()));
        assert_eq!(record.v, Some(22.5));
    }

    #[test]
    fn test_record_with_unit() {
        let record = SenMLRecord::with_value("temperature", 22.5).with_unit("Cel");
        assert_eq!(record.u, Some("Cel".to_string()));
    }

    #[test]
    fn test_string_value_record() {
        let record = SenMLRecord::with_string_value("status", "OK");
        assert_eq!(record.vs, Some("OK".to_string()));
    }

    #[test]
    fn test_bool_value_record() {
        let record = SenMLRecord::with_bool_value("enabled", true);
        assert_eq!(record.vb, Some(true));
    }

    #[test]
    fn test_record_validation() {
        let valid_record = SenMLRecord::with_value("temp", 25.0);
        assert!(valid_record.validate().is_ok());

        let empty_record = SenMLRecord::new();
        assert!(empty_record.validate().is_err());
    }

    #[test]
    fn test_base64_encode_decode() {
        let data = b"hello world";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(data, decoded.as_slice());
    }

    #[test]
    fn test_resolved_name() {
        let record = SenMLRecord::with_value("temp", 25.0);
        assert_eq!(
            record.resolved_name(Some("device1/")),
            Some("device1/temp".to_string())
        );
        assert_eq!(record.resolved_name(None), Some("temp".to_string()));
    }
}
