//! SenML normalization - converting packs to resolved form

use crate::{Result, SenMLError, SenMLPack, SenMLRecord, SenMLValue};
use serde::{Deserialize, Serialize};

/// A normalized SenML pack where all base values have been resolved into individual records
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedPack {
    /// All records in resolved form (no base values except bver)
    pub records: Vec<NormalizedRecord>,
    /// Version information (only base value preserved)
    pub version: Option<i32>,
}

/// A fully resolved SenML record with all base values applied
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRecord {
    /// Full resolved name (base name + record name)
    pub name: String,
    /// Resolved unit (base unit or record unit)
    pub unit: Option<String>,
    /// Resolved numeric value (base value + record value)
    pub value: Option<f64>,
    /// String value (unchanged)
    pub string_value: Option<String>,
    /// Boolean value (unchanged)
    pub bool_value: Option<bool>,
    /// Data value (unchanged)  
    pub data_value: Option<Vec<u8>>,
    /// Resolved sum (base sum + record sum)
    pub sum: Option<f64>,
    /// Resolved timestamp (base time + record time)
    pub time: Option<f64>,
    /// Update time (unchanged)
    pub update_time: Option<f64>,
}

impl NormalizedPack {
    /// Create a normalized pack from a regular SenML pack
    pub fn from_pack(pack: &SenMLPack) -> Self {
        let mut records = Vec::new();

        if pack.records.is_empty() {
            return Self {
                records,
                version: None,
            };
        }

        // Determine if we have base values by checking the first record
        let first_record = &pack.records[0];
        let has_base_name = first_record.n.as_ref().is_some_and(|n| n.ends_with('/'));
        let has_only_base_values = !first_record.has_value() && first_record.s.is_none();
        let is_base_record = has_base_name || has_only_base_values;

        let (base_name, base_time, base_unit, base_value, base_sum) = if is_base_record {
            // Extract base values from the base record
            (
                first_record.n.clone().unwrap_or_default(),
                first_record.t.unwrap_or(0.0),
                first_record.u.clone(),
                first_record.v.unwrap_or(0.0),
                first_record.s.unwrap_or(0.0),
            )
        } else {
            // No base record - use empty base values
            (String::new(), 0.0, None, 0.0, 0.0)
        };

        let start_index = if is_base_record { 1 } else { 0 };

        // Process each record (skip first if it's a base record)
        for record in &pack.records[start_index..] {
            if let Ok(normalized) = Self::normalize_record(
                record, &base_name, base_time, &base_unit, base_value, base_sum,
            ) {
                records.push(normalized);
            }
        }

        // Handle the base record itself if it has values beyond just base values
        // Don't include base records that only contain base values (like base_value=20.0)
        if is_base_record && first_record.has_value() {
            // Check if this is a pure base record (name ends with '/') with only base values
            let is_pure_base = first_record.n.as_ref().is_some_and(|n| n.ends_with('/'));

            // Only include if it's not a pure base record OR if it has sum values
            if !is_pure_base || first_record.s.is_some() {
                if let Ok(normalized) = Self::normalize_record(
                    first_record,
                    "",    // No base name for base record itself
                    0.0,   // No base time
                    &None, // No base unit
                    0.0,   // No base value
                    0.0,   // No base sum
                ) {
                    records.insert(0, normalized);
                }
            }
        }

        Self {
            records,
            version: None, // TODO: Extract from bver if present
        }
    }

    /// Normalize a single record with given base values
    fn normalize_record(
        record: &SenMLRecord,
        base_name: &str,
        base_time: f64,
        base_unit: &Option<String>,
        base_value: f64,
        base_sum: f64,
    ) -> Result<NormalizedRecord> {
        // Resolve name
        let name = match &record.n {
            Some(n) if !base_name.is_empty() => format!("{}{}", base_name, n),
            Some(n) => n.clone(),
            None if !base_name.is_empty() => base_name.to_string(),
            None => return Err(SenMLError::normalization("Record must have a name")),
        };

        // Resolve unit (record unit takes precedence)
        let unit = record.u.clone().or_else(|| base_unit.clone());

        // Resolve numeric value (add base value if both present)
        let value = match (record.v, base_value != 0.0) {
            (Some(v), true) => Some(v + base_value),
            (Some(v), false) => Some(v),
            (None, _) => None,
        };

        // Resolve sum (add base sum if both present)
        let sum = match (record.s, base_sum != 0.0) {
            (Some(s), true) => Some(s + base_sum),
            (Some(s), false) => Some(s),
            (None, _) => None,
        };

        // Resolve time (add base time if record time is relative)
        let time = match (record.t, base_time != 0.0) {
            (Some(t), true) => Some(base_time + t),
            (Some(t), false) => Some(t),
            (None, true) => Some(base_time),
            (None, false) => None,
        };

        // String, boolean, and data values are not affected by base values
        let string_value = record.vs.clone();
        let bool_value = record.vb;
        let data_value = record.vd.as_ref().and_then(|vd| {
            // Decode base64 to actual bytes - ignore errors for now
            base64_decode(vd).ok()
        });

        Ok(NormalizedRecord {
            name,
            unit,
            value,
            string_value,
            bool_value,
            data_value,
            sum,
            time,
            update_time: record.ut,
        })
    }

    /// Convert back to a SenML pack (may not preserve original base structure)
    pub fn to_pack(&self) -> SenMLPack {
        let records: Vec<SenMLRecord> = self
            .records
            .iter()
            .map(|nr| SenMLRecord {
                n: Some(nr.name.clone()),
                u: nr.unit.clone(),
                v: nr.value,
                vs: nr.string_value.clone(),
                vb: nr.bool_value,
                vd: nr.data_value.as_ref().map(|data| base64_encode(data)),
                s: nr.sum,
                t: nr.time,
                ut: nr.update_time,
            })
            .collect();

        SenMLPack { records }
    }

    /// Get all records with a specific name pattern
    pub fn records_matching(&self, pattern: &str) -> Vec<&NormalizedRecord> {
        self.records
            .iter()
            .filter(|record| record.name.contains(pattern))
            .collect()
    }

    /// Get all records within a time range
    pub fn records_in_time_range(&self, start: f64, end: f64) -> Vec<&NormalizedRecord> {
        self.records
            .iter()
            .filter(|record| {
                if let Some(time) = record.time {
                    time >= start && time <= end
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get the time range of this pack
    pub fn time_range(&self) -> Option<(f64, f64)> {
        let times: Vec<f64> = self.records.iter().filter_map(|r| r.time).collect();

        if times.is_empty() {
            None
        } else {
            let min_time = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max_time = times.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
            Some((min_time, max_time))
        }
    }

    /// Group records by name prefix
    pub fn group_by_prefix(&self) -> std::collections::HashMap<String, Vec<&NormalizedRecord>> {
        let mut groups = std::collections::HashMap::new();

        for record in &self.records {
            // Extract prefix (everything before the last '/')
            let prefix = if let Some(pos) = record.name.rfind('/') {
                record.name[..pos].to_string()
            } else {
                "".to_string()
            };

            groups.entry(prefix).or_insert_with(Vec::new).push(record);
        }

        groups
    }

    /// Validate the normalized pack
    pub fn validate(&self) -> Result<()> {
        for (i, record) in self.records.iter().enumerate() {
            record.validate().map_err(|e| {
                SenMLError::validation(format!("Invalid normalized record at index {}: {}", i, e))
            })?;
        }
        Ok(())
    }
}

impl NormalizedRecord {
    /// Get the primary value from this record
    pub fn primary_value(&self) -> Option<SenMLValue> {
        if let Some(v) = self.value {
            Some(SenMLValue::Number(v))
        } else if let Some(ref vs) = self.string_value {
            Some(SenMLValue::String(vs.clone()))
        } else if let Some(vb) = self.bool_value {
            Some(SenMLValue::Boolean(vb))
        } else {
            self.data_value
                .as_ref()
                .map(|vd| SenMLValue::Data(vd.clone()))
        }
    }

    /// Check if this record has any value
    pub fn has_value(&self) -> bool {
        self.value.is_some()
            || self.string_value.is_some()
            || self.bool_value.is_some()
            || self.data_value.is_some()
    }

    /// Get the base name (everything up to last '/')
    pub fn base_name(&self) -> Option<&str> {
        self.name.rfind('/').map(|pos| &self.name[..pos + 1])
    }

    /// Get the local name (everything after last '/')
    pub fn local_name(&self) -> &str {
        if let Some(pos) = self.name.rfind('/') {
            &self.name[pos + 1..]
        } else {
            &self.name
        }
    }

    /// Validate this normalized record
    pub fn validate(&self) -> Result<()> {
        // Must have a name
        if self.name.is_empty() {
            return Err(SenMLError::validation("Normalized record must have a name"));
        }

        // Must have at least one value or sum
        if !self.has_value() && self.sum.is_none() {
            return Err(SenMLError::validation(
                "Normalized record must have at least one value field",
            ));
        }

        // Validate numeric values
        if let Some(v) = self.value {
            if !v.is_finite() {
                return Err(SenMLError::invalid_field_value("value", &v.to_string()));
            }
        }

        if let Some(s) = self.sum {
            if !s.is_finite() {
                return Err(SenMLError::invalid_field_value("sum", &s.to_string()));
            }
        }

        if let Some(t) = self.time {
            if !t.is_finite() {
                return Err(SenMLError::invalid_field_value("time", &t.to_string()));
            }
        }

        if let Some(ut) = self.update_time {
            if !ut.is_finite() || ut < 0.0 {
                return Err(SenMLError::invalid_field_value(
                    "update_time",
                    &ut.to_string(),
                ));
            }
        }

        Ok(())
    }
}

// Helper functions for base64 encoding/decoding (reused from record.rs)
fn base64_encode(data: &[u8]) -> String {
    // Same implementation as in record.rs
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

fn base64_decode(s: &str) -> std::result::Result<Vec<u8>, &'static str> {
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
    use crate::{SenMLBuilder, SenMLRecord};

    #[test]
    fn test_basic_normalization() {
        let pack = SenMLBuilder::new()
            .base_name("device1/")
            .base_time(1640995200.0)
            .base_unit("Cel")
            .add_value("temp", 22.5)
            .build();

        let normalized = pack.normalize();

        assert_eq!(normalized.records.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.name, "device1/temp");
        assert_eq!(record.value, Some(22.5));
        assert_eq!(record.unit, Some("Cel".to_string()));
        assert_eq!(record.time, Some(1640995200.0));
    }

    #[test]
    fn test_normalization_with_base_values() {
        let pack = SenMLBuilder::new()
            .base_name("sensor/")
            .base_time(1000.0)
            .base_value(20.0)
            .add_measurement("temp", 2.5, 60.0) // Should become 22.5 at time 1060.0
            .build();

        let normalized = pack.normalize();

        assert_eq!(normalized.records.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.name, "sensor/temp");
        assert_eq!(record.value, Some(22.5)); // 20.0 + 2.5
        assert_eq!(record.time, Some(1060.0)); // 1000.0 + 60.0
    }

    #[test]
    fn test_normalization_without_base_record() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("standalone", 42.0));

        let normalized = pack.normalize();

        assert_eq!(normalized.records.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.name, "standalone");
        assert_eq!(record.value, Some(42.0));
    }

    #[test]
    fn test_time_range() {
        let pack = SenMLBuilder::new()
            .add_measurement("temp1", 20.0, 100.0)
            .add_measurement("temp2", 25.0, 200.0)
            .add_measurement("temp3", 30.0, 150.0)
            .build();

        let normalized = pack.normalize();
        let range = normalized.time_range();

        assert_eq!(range, Some((100.0, 200.0)));
    }

    #[test]
    fn test_records_in_time_range() {
        let pack = SenMLBuilder::new()
            .add_measurement("temp1", 20.0, 100.0)
            .add_measurement("temp2", 25.0, 200.0)
            .add_measurement("temp3", 30.0, 300.0)
            .build();

        let normalized = pack.normalize();
        let filtered = normalized.records_in_time_range(150.0, 250.0);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "temp2");
    }

    #[test]
    fn test_group_by_prefix() {
        let pack = SenMLBuilder::new()
            .add_value("device1/temp", 20.0)
            .add_value("device1/humidity", 50.0)
            .add_value("device2/temp", 25.0)
            .build();

        let normalized = pack.normalize();
        let groups = normalized.group_by_prefix();

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("device1"));
        assert!(groups.contains_key("device2"));
        assert_eq!(groups["device1"].len(), 2);
        assert_eq!(groups["device2"].len(), 1);
    }

    #[test]
    fn test_local_and_base_name() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("device1/sensor/temperature", 22.5));

        let normalized = pack.normalize();
        let record = &normalized.records[0];

        assert_eq!(record.base_name(), Some("device1/sensor/"));
        assert_eq!(record.local_name(), "temperature");
    }

    #[test]
    fn test_normalization_validation() {
        let pack = SenMLBuilder::new().add_value("valid", 25.0).build();

        let normalized = pack.normalize();
        assert!(normalized.validate().is_ok());
    }

    #[test]
    fn test_roundtrip_normalization() {
        let original = SenMLBuilder::new()
            .add_value("temp", 22.5)
            .add_string_value("status", "OK")
            .build();

        let normalized = original.normalize();
        let restored = normalized.to_pack();

        // Should have same number of records (though structure may differ)
        assert_eq!(restored.records.len(), original.records.len());
    }
}
