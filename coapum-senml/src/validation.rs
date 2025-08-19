//! Validation support for SenML data according to RFC 8428

use crate::{SenMLPack, SenMLRecord, NormalizedPack, Result, SenMLError};

/// Trait for validating SenML data structures
pub trait Validate {
    /// Validate this item according to RFC 8428 rules
    fn validate(&self) -> Result<()>;
}

impl Validate for SenMLPack {
    fn validate(&self) -> Result<()> {
        self.validate()
    }
}

impl Validate for SenMLRecord {
    fn validate(&self) -> Result<()> {
        self.validate()
    }
}

impl Validate for NormalizedPack {
    fn validate(&self) -> Result<()> {
        self.validate()
    }
}

/// Time threshold for relative vs absolute time (RFC 8428)
const TIME_THRESHOLD: f64 = 268435456.0; // 2^28

/// Default SenML version (RFC 8428)
const DEFAULT_SENML_VERSION: i32 = 10;

/// Comprehensive validation for SenML packs
pub struct PackValidator {
    /// Whether to allow empty packs (default: false)
    pub allow_empty: bool,
    /// Whether to enforce strict name requirements
    pub strict_names: bool,
    /// Maximum allowed time drift for timestamps
    pub max_time_drift: Option<f64>,
    /// Required units for specific measurements
    pub required_units: std::collections::HashMap<String, String>,
    /// Enforce RFC 8428 strict compliance
    pub rfc_strict: bool,
}

impl Default for PackValidator {
    fn default() -> Self {
        Self {
            allow_empty: false,
            strict_names: true,
            max_time_drift: None,
            required_units: std::collections::HashMap::new(),
            rfc_strict: true,
        }
    }
}

impl PackValidator {
    /// Create a new validator with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow empty packs (useful for some applications)
    pub fn allow_empty(mut self) -> Self {
        self.allow_empty = true;
        self
    }

    /// Enable strict name validation (URIs, no spaces, etc.)
    pub fn strict_names(mut self, strict: bool) -> Self {
        self.strict_names = strict;
        self
    }

    /// Set maximum allowed time drift for timestamp validation
    pub fn max_time_drift(mut self, drift: f64) -> Self {
        self.max_time_drift = Some(drift);
        self
    }

    /// Require specific units for measurements
    pub fn require_unit<S1: Into<String>, S2: Into<String>>(
        mut self, 
        measurement: S1, 
        unit: S2
    ) -> Self {
        self.required_units.insert(measurement.into(), unit.into());
        self
    }

    /// Enable RFC 8428 strict compliance mode
    pub fn rfc_strict(mut self, strict: bool) -> Self {
        self.rfc_strict = strict;
        self
    }

    /// Validate a SenML pack with these settings
    pub fn validate_pack(&self, pack: &SenMLPack) -> Result<()> {
        // Basic validation first
        pack.validate()?;

        // Check empty pack rule
        if !self.allow_empty && pack.is_empty() {
            return Err(SenMLError::validation("Empty pack not allowed"));
        }

        // RFC 8428 strict compliance checks
        if self.rfc_strict {
            self.validate_rfc_compliance(pack)?;
        }

        // Validate each record with extended rules
        for (i, record) in pack.iter().enumerate() {
            self.validate_record(record)
                .map_err(|e| SenMLError::validation(
                    format!("Record {} validation failed: {}", i, e)
                ))?;
        }

        // Cross-record validation
        self.validate_pack_consistency(pack)?;

        Ok(())
    }

    /// Validate a single record with extended rules
    pub fn validate_record(&self, record: &SenMLRecord) -> Result<()> {
        // Basic record validation
        record.validate()?;

        // Strict name validation
        if self.strict_names {
            if let Some(ref name) = record.n {
                self.validate_name(name)?;
            }
        }

        // Unit requirements
        if let Some(ref name) = record.n {
            if let Some(required_unit) = self.required_units.get(name) {
                match &record.u {
                    Some(unit) if unit == required_unit => {}, // OK
                    Some(unit) => return Err(SenMLError::validation(
                        format!("Measurement '{}' requires unit '{}', got '{}'", 
                               name, required_unit, unit)
                    )),
                    None => return Err(SenMLError::validation(
                        format!("Measurement '{}' requires unit '{}'", 
                               name, required_unit)
                    )),
                }
            }
        }

        Ok(())
    }

    /// Validate measurement name according to strict rules
    fn validate_name(&self, name: &str) -> Result<()> {
        // Must not be empty
        if name.is_empty() {
            return Err(SenMLError::validation("Name cannot be empty"));
        }

        // Check for invalid characters (basic URI safety)
        if name.contains(' ') || name.contains('\t') || name.contains('\n') {
            return Err(SenMLError::validation("Name contains invalid whitespace"));
        }

        // Check for control characters
        if name.chars().any(|c| c.is_control()) {
            return Err(SenMLError::validation("Name contains control characters"));
        }

        // Maximum length (reasonable limit)
        if name.len() > 256 {
            return Err(SenMLError::validation("Name too long (max 256 characters)"));
        }

        Ok(())
    }

    /// Validate pack-wide consistency
    fn validate_pack_consistency(&self, pack: &SenMLPack) -> Result<()> {
        if pack.is_empty() {
            return Ok(());
        }

        // Check for duplicate names at same timestamp
        let mut seen_entries = std::collections::HashSet::new();
        
        for record in pack.iter() {
            if let Some(ref name) = record.n {
                let time = record.t.unwrap_or(0.0);
                let entry = (name.clone(), time as i64); // Use integer for floating point comparison
                
                if seen_entries.contains(&entry) {
                    return Err(SenMLError::validation(
                        format!("Duplicate entry for '{}' at time {}", name, time)
                    ));
                }
                seen_entries.insert(entry);
            }
        }

        // Validate time ordering if requested
        if let Some(max_drift) = self.max_time_drift {
            self.validate_time_ordering(pack, max_drift)?;
        }

        Ok(())
    }

    /// Validate that timestamps are reasonably ordered
    fn validate_time_ordering(&self, pack: &SenMLPack, max_drift: f64) -> Result<()> {
        let mut last_time: Option<f64> = None;
        
        for record in pack.iter() {
            if let Some(time) = record.t {
                if let Some(prev_time) = last_time {
                    // Check for excessive backward drift
                    if prev_time - time > max_drift {
                        return Err(SenMLError::validation(
                            format!("Time goes backward by {:.2}s (max drift: {:.2}s)", 
                                   prev_time - time, max_drift)
                        ));
                    }
                }
                last_time = Some(time);
            }
        }
        
        Ok(())
    }

    /// Validate RFC 8428 strict compliance
    fn validate_rfc_compliance(&self, pack: &SenMLPack) -> Result<()> {
        if pack.is_empty() {
            return Ok(());
        }

        // Check Base Version (bver) - RFC 8428 Section 4.1
        let base_values = pack.base_values();
        let version = base_values.bver.unwrap_or(DEFAULT_SENML_VERSION);
        if version != DEFAULT_SENML_VERSION {
            return Err(SenMLError::validation(
                format!("Unsupported SenML version: {} (expected: {})", 
                       version, DEFAULT_SENML_VERSION)
            ));
        }

        // Validate time handling - RFC 8428 Section 4.5.2
        for record in pack.iter() {
            if let Some(time) = record.t {
                self.validate_time_value(time)?;
            }
        }

        // Check for fields ending with "_" - RFC 8428 Section 4.1
        for record in pack.iter() {
            if let Some(ref name) = record.n {
                self.validate_field_names(name)?;
            }
        }

        // Validate IEEE double-precision compliance
        for record in pack.iter() {
            self.validate_ieee_compliance(record)?;
        }

        Ok(())
    }

    /// Validate time value according to RFC 8428
    fn validate_time_value(&self, time: f64) -> Result<()> {
        if !time.is_finite() {
            return Err(SenMLError::validation("Time value must be finite"));
        }

        // RFC 8428: Time values < 2^28 are relative, >= 2^28 are absolute
        if time >= TIME_THRESHOLD {
            // Absolute time - should be reasonable Unix timestamp
            if time < 0.0 {
                return Err(SenMLError::validation(
                    "Absolute time values must be positive"
                ));
            }
        }
        // Relative times can be negative (past events)

        Ok(())
    }

    /// Validate field names don't use reserved patterns
    fn validate_field_names(&self, name: &str) -> Result<()> {
        // RFC 8428: Fields ending with "_" are reserved
        if name.ends_with('_') {
            return Err(SenMLError::validation(
                format!("Field name '{}' ends with reserved '_' character", name)
            ));
        }
        Ok(())
    }

    /// Validate IEEE double-precision compliance
    fn validate_ieee_compliance(&self, record: &SenMLRecord) -> Result<()> {
        // Check all numeric fields are valid IEEE 754 double-precision
        if let Some(v) = record.v {
            if !v.is_finite() && !v.is_nan() {
                return Err(SenMLError::validation(
                    "Numeric values must be finite or NaN (IEEE 754)"
                ));
            }
        }

        if let Some(s) = record.s {
            if !s.is_finite() && !s.is_nan() {
                return Err(SenMLError::validation(
                    "Sum values must be finite or NaN (IEEE 754)"
                ));
            }
        }

        if let Some(t) = record.t {
            if !t.is_finite() {
                return Err(SenMLError::validation(
                    "Time values must be finite (IEEE 754)"
                ));
            }
        }

        if let Some(ut) = record.ut {
            if !ut.is_finite() || ut < 0.0 {
                return Err(SenMLError::validation(
                    "Update time must be finite and non-negative"
                ));
            }
        }

        Ok(())
    }
}

/// Specific validators for common use cases
pub mod validators {
    use super::*;

    /// Validator for IoT sensor data
    pub fn iot_sensor() -> PackValidator {
        PackValidator::new()
            .strict_names(true)
            .max_time_drift(3600.0) // Allow 1 hour backward drift
            .require_unit("temperature", "Cel")
            .require_unit("humidity", "%RH")
            .require_unit("pressure", "Pa")
    }

    /// Validator for energy monitoring
    pub fn energy_monitor() -> PackValidator {
        PackValidator::new()
            .strict_names(true)
            .require_unit("power", "W")
            .require_unit("energy", "Wh")
            .require_unit("voltage", "V")
            .require_unit("current", "A")
    }

    /// Relaxed validator for development/testing
    pub fn relaxed() -> PackValidator {
        PackValidator::new()
            .allow_empty()
            .strict_names(false)
    }

    /// Strict validator for production systems
    pub fn production() -> PackValidator {
        PackValidator::new()
            .strict_names(true)
            .max_time_drift(300.0) // 5 minutes max drift
    }

    /// RFC 8428 compliant validator
    pub fn rfc8428_compliant() -> PackValidator {
        PackValidator::new()
            .rfc_strict(true)
            .strict_names(true)
    }
}

/// Validation utilities
pub mod utils {
    use super::*;

    /// Check if a string is a valid SenML name
    pub fn is_valid_name(name: &str) -> bool {
        PackValidator::new().validate_name(name).is_ok()
    }

    /// Check if a unit string is valid SI unit
    pub fn is_valid_unit(unit: &str) -> bool {
        // Basic unit validation - in production you'd have a comprehensive list
        !unit.is_empty() && 
        !unit.contains(' ') && 
        unit.chars().all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '%')
    }

    /// Suggest corrections for common unit mistakes
    pub fn suggest_unit_correction(unit: &str) -> Option<&'static str> {
        match unit.to_lowercase().as_str() {
            "celsius" | "°c" | "degc" => Some("Cel"),
            "fahrenheit" | "°f" | "degf" => Some("degF"), 
            "percent" | "percentage" => Some("%"),
            "watts" | "watt" => Some("W"),
            "volts" | "volt" => Some("V"),
            "amps" | "ampere" | "amperes" => Some("A"),
            "pascal" | "pascals" => Some("Pa"),
            "seconds" | "second" | "sec" => Some("s"),
            "meters" | "meter" | "metre" => Some("m"),
            _ => None,
        }
    }

    /// Validate timestamp is reasonable (not too far in past/future)
    pub fn is_reasonable_timestamp(timestamp: f64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        
        // Allow 100 years in past, 10 years in future
        let min_time = now - (100.0 * 365.25 * 24.0 * 3600.0);
        let max_time = now + (10.0 * 365.25 * 24.0 * 3600.0);
        
        timestamp >= min_time && timestamp <= max_time
    }

    /// Check if time value is relative (< 2^28) or absolute (>= 2^28)
    pub fn is_relative_time(time: f64) -> bool {
        time < TIME_THRESHOLD
    }

    /// Check if time value is absolute Unix timestamp
    pub fn is_absolute_time(time: f64) -> bool {
        time >= TIME_THRESHOLD
    }

    /// Validate field name doesn't use reserved patterns
    pub fn is_valid_field_name(name: &str) -> bool {
        !name.ends_with('_') && !name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SenMLBuilder, SenMLRecord, SenMLPack};
    use super::{validators, utils, TIME_THRESHOLD};

    #[test]
    fn test_basic_validation() {
        let pack = SenMLBuilder::new()
            .add_value("temperature", 22.5)
            .build();

        let validator = PackValidator::new().allow_empty();
        assert!(validator.validate_pack(&pack).is_ok());
    }

    #[test]
    fn test_empty_pack_validation() {
        let pack = SenMLPack::new();
        
        let strict_validator = PackValidator::new();
        assert!(strict_validator.validate_pack(&pack).is_err());

        let relaxed_validator = PackValidator::new().allow_empty();
        assert!(relaxed_validator.validate_pack(&pack).is_ok());
    }

    #[test]
    fn test_unit_requirements() {
        let pack = SenMLBuilder::new()
            .add_measurement_with_unit("temperature", 22.5, "Cel", 0.0)
            .build();

        let validator = PackValidator::new()
            .require_unit("temperature", "Cel");
        assert!(validator.validate_pack(&pack).is_ok());

        let wrong_unit_pack = SenMLBuilder::new()
            .add_measurement_with_unit("temperature", 22.5, "F", 0.0)
            .build();
        assert!(validator.validate_pack(&wrong_unit_pack).is_err());
    }

    #[test]
    fn test_strict_name_validation() {
        let validator = PackValidator::new().strict_names(true);

        // Valid names
        assert!(validator.validate_name("temperature").is_ok());
        assert!(validator.validate_name("device1/temp").is_ok());
        assert!(validator.validate_name("sensor_01").is_ok());

        // Invalid names  
        assert!(validator.validate_name("").is_err()); // Empty
        assert!(validator.validate_name("temp with spaces").is_err()); // Spaces
        assert!(validator.validate_name("temp\twith\ttabs").is_err()); // Tabs
    }

    #[test]
    fn test_duplicate_detection() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 20.0).with_time(100.0));
        pack.add_record(SenMLRecord::with_value("temp", 21.0).with_time(100.0)); // Duplicate

        let validator = PackValidator::new();
        assert!(validator.validate_pack(&pack).is_err());
    }

    #[test]
    fn test_time_drift_validation() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp1", 20.0).with_time(1000.0));
        pack.add_record(SenMLRecord::with_value("temp2", 21.0).with_time(500.0)); // Goes backward

        let validator = PackValidator::new().max_time_drift(100.0);
        assert!(validator.validate_pack(&pack).is_err());

        let lenient_validator = PackValidator::new().max_time_drift(1000.0);
        assert!(lenient_validator.validate_pack(&pack).is_ok());
    }

    #[test]
    fn test_iot_sensor_validator() {
        let pack = SenMLBuilder::new()
            .add_measurement_with_unit("temperature", 22.5, "Cel", 1000.0)
            .add_measurement_with_unit("humidity", 45.0, "%RH", 1001.0)
            .build();

        let validator = validators::iot_sensor();
        assert!(validator.validate_pack(&pack).is_ok());
    }

    #[test]
    fn test_utils_name_validation() {
        assert!(utils::is_valid_name("temperature"));
        assert!(utils::is_valid_name("device1/sensor/temp"));
        assert!(!utils::is_valid_name(""));
        assert!(!utils::is_valid_name("temp with spaces"));
    }

    #[test]
    fn test_utils_unit_suggestions() {
        assert_eq!(utils::suggest_unit_correction("celsius"), Some("Cel"));
        assert_eq!(utils::suggest_unit_correction("watts"), Some("W"));
        assert_eq!(utils::suggest_unit_correction("unknown_unit"), None);
    }

    #[test]
    fn test_utils_timestamp_validation() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        
        assert!(utils::is_reasonable_timestamp(now));
        assert!(utils::is_reasonable_timestamp(now - 3600.0)); // 1 hour ago
        assert!(!utils::is_reasonable_timestamp(0.0)); // Too old
        assert!(!utils::is_reasonable_timestamp(now + 365.25 * 24.0 * 3600.0 * 50.0)); // 50 years future
    }

    #[test]
    fn test_rfc8428_time_classification() {
        // Test relative vs absolute time classification
        assert!(utils::is_relative_time(1000.0)); // Relative
        assert!(utils::is_relative_time(-60.0)); // Relative (past)
        assert!(utils::is_absolute_time(1640995200.0)); // Absolute Unix time
        assert!(utils::is_absolute_time(TIME_THRESHOLD)); // Boundary
    }

    #[test]
    fn test_rfc8428_field_names() {
        assert!(utils::is_valid_field_name("temperature"));
        assert!(utils::is_valid_field_name("sensor/temp"));
        assert!(!utils::is_valid_field_name("reserved_")); // Ends with _
        assert!(!utils::is_valid_field_name("")); // Empty
    }

    #[test]
    fn test_rfc8428_compliance_validator() {
        let validator = validators::rfc8428_compliant();
        
        // Valid pack should pass
        let pack = SenMLBuilder::new()
            .add_value("temp", 22.5)
            .build();
        assert!(validator.validate_pack(&pack).is_ok());

        // Pack with invalid field name should fail
        let mut invalid_pack = SenMLPack::new();
        invalid_pack.add_record(SenMLRecord::with_value("invalid_", 25.0));
        assert!(validator.validate_pack(&invalid_pack).is_err());
    }
}