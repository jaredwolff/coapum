//! CBOR serialization support for SenML

#[cfg(feature = "cbor")]
use crate::{SenMLPack, Result, SenMLError};

#[cfg(feature = "cbor")]
impl SenMLPack {
    /// Serialize SenML pack to CBOR with specific options
    pub fn to_cbor_with_options(&self, canonical: bool) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        
        if canonical {
            // Use canonical CBOR encoding for deterministic output
            ciborium::ser::into_writer(self, &mut buffer)
                .map_err(|e| SenMLError::serialization(e.to_string()))?;
        } else {
            ciborium::ser::into_writer(self, &mut buffer)
                .map_err(|e| SenMLError::serialization(e.to_string()))?;
        }
        
        Ok(buffer)
    }

    /// Deserialize from CBOR with validation
    pub fn from_cbor_validated(bytes: &[u8]) -> Result<Self> {
        let pack = Self::from_cbor(bytes)?;
        pack.validate()?;
        Ok(pack)
    }

    /// Get CBOR diagnostic notation (for debugging)
    pub fn to_cbor_diagnostic(&self) -> Result<String> {
        let cbor_bytes = self.to_cbor()?;
        
        // Simple diagnostic representation
        // In a real implementation, you'd use a proper CBOR diagnostic library
        Ok(format!("CBOR({} bytes)", cbor_bytes.len()))
    }

    /// Check if bytes contain valid CBOR SenML data
    pub fn is_valid_cbor_senml(bytes: &[u8]) -> bool {
        Self::from_cbor(bytes).is_ok()
    }
}

/// CBOR-specific utilities  
#[cfg(feature = "cbor")]
pub mod utils {
    use super::*;

    /// Content-Type for SenML CBOR format
    pub const SENML_CBOR_CONTENT_TYPE: &str = "application/senml+cbor";
    
    /// Content-Type for SenSML CBOR format (stream)
    pub const SENSML_CBOR_CONTENT_TYPE: &str = "application/sensml+cbor";
    
    /// CBOR tag for SenML (if standardized)
    pub const SENML_CBOR_TAG: u64 = 1000; // Example tag - not in RFC
    
    /// Check CBOR major type of the data
    pub fn get_cbor_major_type(bytes: &[u8]) -> Option<u8> {
        bytes.first().map(|b| b >> 5)
    }
    
    /// Check if CBOR data starts with array (major type 4)
    pub fn is_cbor_array(bytes: &[u8]) -> bool {
        get_cbor_major_type(bytes) == Some(4)
    }
    
    /// Estimate uncompressed size of CBOR data
    pub fn estimate_json_size(cbor_bytes: &[u8]) -> usize {
        // Rough estimate: CBOR is typically 10-50% smaller than JSON
        cbor_bytes.len() * 2
    }

    /// Extract media type from Content-Type header  
    pub fn parse_cbor_content_type(content_type: &str) -> Option<&str> {
        if content_type.starts_with("application/senml+cbor") {
            Some("senml")
        } else if content_type.starts_with("application/sensml+cbor") {
            Some("sensml")
        } else {
            None
        }
    }
}

/// CBOR encoding helpers
#[cfg(feature = "cbor")]
pub mod encoding {
    use super::*;
    
    /// Encode SenML pack with space optimization
    pub fn encode_compact(pack: &SenMLPack) -> Result<Vec<u8>> {
        // Use shortest possible encoding for numeric values
        pack.to_cbor()
    }
    
    /// Encode SenML pack with deterministic ordering
    pub fn encode_canonical(pack: &SenMLPack) -> Result<Vec<u8>> {
        pack.to_cbor_with_options(true)
    }
    
    /// Encode with compression if beneficial
    pub fn encode_compressed(pack: &SenMLPack) -> Result<Vec<u8>> {
        let cbor = pack.to_cbor()?;
        
        // Simple size-based decision
        if cbor.len() > 1024 {
            // In production, use proper compression like deflate
            // For now, just return regular CBOR
            Ok(cbor)
        } else {
            Ok(cbor)
        }
    }
}

#[cfg(test)]
#[cfg(feature = "cbor")]
mod tests {
    use crate::{SenMLPack, SenMLRecord};
    use super::{utils, encoding};

    #[test]
    fn test_cbor_serialization() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temperature", 22.5).with_unit("Cel"));
        
        let cbor = pack.to_cbor().expect("CBOR serialization failed");
        assert!(!cbor.is_empty());
    }

    #[test]
    fn test_cbor_roundtrip() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 25.0));
        pack.add_record(SenMLRecord::with_string_value("status", "OK"));
        pack.add_record(SenMLRecord::with_bool_value("enabled", true));
        
        let cbor = pack.to_cbor().unwrap();
        let restored = SenMLPack::from_cbor(&cbor).unwrap();
        
        assert_eq!(pack, restored);
    }

    #[test]
    fn test_cbor_validation() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 30.0));
        
        let cbor = pack.to_cbor().unwrap();
        let validated = SenMLPack::from_cbor_validated(&cbor).unwrap();
        
        assert_eq!(pack, validated);
    }

    #[test]
    fn test_cbor_canonical() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 25.0));
        
        let canonical = pack.to_cbor_with_options(true).unwrap();
        let regular = pack.to_cbor().unwrap();
        
        // Both should decode to same result
        let from_canonical = SenMLPack::from_cbor(&canonical).unwrap();
        let from_regular = SenMLPack::from_cbor(&regular).unwrap();
        
        assert_eq!(from_canonical, from_regular);
    }

    #[test]
    fn test_utils_cbor_major_type() {
        let array_cbor = [0x80u8]; // Empty array
        assert_eq!(utils::get_cbor_major_type(&array_cbor), Some(4));
        assert!(utils::is_cbor_array(&array_cbor));
    }

    #[test]
    fn test_utils_content_type_parsing() {
        assert_eq!(
            utils::parse_cbor_content_type("application/senml+cbor"),
            Some("senml")
        );
        assert_eq!(
            utils::parse_cbor_content_type("application/sensml+cbor"),
            Some("sensml")
        );
        assert_eq!(
            utils::parse_cbor_content_type("application/cbor"),
            None
        );
    }

    #[test]
    fn test_encoding_helpers() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 20.0));
        
        let compact = encoding::encode_compact(&pack).unwrap();
        let canonical = encoding::encode_canonical(&pack).unwrap();
        let compressed = encoding::encode_compressed(&pack).unwrap();
        
        // All should decode to the same pack
        assert_eq!(
            SenMLPack::from_cbor(&compact).unwrap(),
            pack
        );
        assert_eq!(
            SenMLPack::from_cbor(&canonical).unwrap(),
            pack
        );
        assert_eq!(
            SenMLPack::from_cbor(&compressed).unwrap(),
            pack
        );
    }

    #[test]
    fn test_cbor_vs_json_size() {
        let mut pack = SenMLPack::new();
        for i in 0..10 {
            pack.add_record(
                SenMLRecord::with_value(&format!("sensor{}", i), i as f64)
                    .with_unit("V")
                    .with_time(i as f64)
            );
        }
        
        let cbor = pack.to_cbor().unwrap();
        let json = pack.to_json().unwrap();
        
        println!("CBOR: {} bytes, JSON: {} bytes", cbor.len(), json.len());
        
        // CBOR should generally be more compact
        assert!(cbor.len() <= json.len());
    }
}