//! JSON serialization support for SenML

#[cfg(feature = "json")]
use crate::{SenMLPack, Result, SenMLError};

#[cfg(feature = "json")]
impl SenMLPack {
    /// Serialize SenML pack to JSON bytes
    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| SenMLError::serialization(e.to_string()))
    }

    /// Deserialize SenML pack from JSON bytes  
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes)
            .map_err(|e| SenMLError::deserialization(e.to_string()))
    }

    /// Serialize to compact JSON (no whitespace)
    pub fn to_json_compact(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| SenMLError::serialization(e.to_string()))
    }

    /// Validate JSON string contains valid SenML
    pub fn validate_json(json: &str) -> Result<()> {
        let pack = Self::from_json(json)?;
        pack.validate()
    }
}

/// JSON-specific utilities
#[cfg(feature = "json")]
pub mod utils {
    use super::*;
    
    /// Content-Type for SenML JSON format
    pub const SENML_JSON_CONTENT_TYPE: &str = "application/senml+json";
    
    /// Content-Type for SenSML JSON format (stream)
    pub const SENSML_JSON_CONTENT_TYPE: &str = "application/sensml+json";
    
    /// Check if a string looks like valid SenML JSON
    pub fn is_senml_json(data: &str) -> bool {
        // Quick heuristic check before full parsing
        data.trim_start().starts_with('[') && 
        data.contains("\"n\"") || data.contains("\"v\"") || data.contains("\"vs\"")
    }
    
    /// Extract media type parameters from Content-Type header
    pub fn parse_content_type(content_type: &str) -> Option<&str> {
        if content_type.starts_with("application/senml+json") {
            Some("senml")
        } else if content_type.starts_with("application/sensml+json") {
            Some("sensml") 
        } else {
            None
        }
    }
}

#[cfg(test)]
#[cfg(feature = "json")]
mod tests {
    use crate::{SenMLPack, SenMLRecord};
    use super::utils;

    #[test]
    fn test_json_serialization() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temperature", 22.5).with_unit("Cel"));
        
        let json = pack.to_json().expect("JSON serialization failed");
        assert!(json.contains("temperature"));
        assert!(json.contains("22.5"));
        assert!(json.contains("Cel"));
    }

    #[test]
    fn test_json_roundtrip() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 25.0));
        pack.add_record(SenMLRecord::with_string_value("status", "OK"));
        pack.add_record(SenMLRecord::with_bool_value("enabled", true));
        
        let json = pack.to_json().unwrap();
        let restored = SenMLPack::from_json(&json).unwrap();
        
        assert_eq!(pack, restored);
    }

    #[test]
    fn test_json_compact() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 20.0));
        
        let compact = pack.to_json_compact().unwrap();
        let pretty = pack.to_json_pretty().unwrap();
        
        assert!(compact.len() < pretty.len());
        assert!(!compact.contains('\n'));
    }

    #[test]
    fn test_json_bytes() {
        let mut pack = SenMLPack::new();
        pack.add_record(SenMLRecord::with_value("temp", 30.0));
        
        let bytes = pack.to_json_bytes().unwrap();
        let restored = SenMLPack::from_json_bytes(&bytes).unwrap();
        
        assert_eq!(pack, restored);
    }

    #[test]
    fn test_utils_is_senml_json() {
        assert!(utils::is_senml_json(r#"[{"n":"temp","v":25.0}]"#));
        assert!(utils::is_senml_json(r#"[{"n":"status","vs":"OK"}]"#));
        assert!(!utils::is_senml_json(r#"{"not":"senml"}"#));
        assert!(!utils::is_senml_json(r#"invalid json"#));
    }

    #[test]
    fn test_utils_parse_content_type() {
        assert_eq!(
            utils::parse_content_type("application/senml+json"),
            Some("senml")
        );
        assert_eq!(
            utils::parse_content_type("application/sensml+json; charset=utf-8"),
            Some("sensml")
        );
        assert_eq!(
            utils::parse_content_type("application/json"),
            None
        );
    }

    #[test]
    fn test_json_validation() {
        let valid_json = r#"[{"n":"temp","v":25.0}]"#;
        assert!(SenMLPack::validate_json(valid_json).is_ok());
        
        let invalid_json = r#"[{"invalid":"structure"}]"#;
        assert!(SenMLPack::validate_json(invalid_json).is_err());
    }
}