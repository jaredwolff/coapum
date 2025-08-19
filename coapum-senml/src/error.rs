//! Error types for SenML operations

use thiserror::Error;

/// Result type alias for SenML operations
pub type Result<T> = std::result::Result<T, SenMLError>;

/// Errors that can occur during SenML operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum SenMLError {
    /// Invalid SenML structure or data
    #[error("Invalid SenML data: {message}")]
    InvalidData { message: String },

    /// Missing required field
    #[error("Missing required field: {field}")]
    MissingField { field: String },

    /// Invalid field value
    #[error("Invalid value for field '{field}': {value}")]
    InvalidFieldValue { field: String, value: String },

    /// Validation error
    #[error("Validation failed: {message}")]
    ValidationError { message: String },

    /// Serialization error
    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    /// Deserialization error
    #[error("Deserialization error: {message}")]
    DeserializationError { message: String },

    /// Time-related error
    #[error("Time error: {message}")]
    TimeError { message: String },

    /// Unit conversion error
    #[error("Unit conversion error: from '{from}' to '{to}'")]
    UnitConversionError { from: String, to: String },

    /// Record normalization error
    #[error("Normalization error: {message}")]
    NormalizationError { message: String },
}

impl SenMLError {
    /// Create an invalid data error
    pub fn invalid_data<S: Into<String>>(message: S) -> Self {
        Self::InvalidData {
            message: message.into(),
        }
    }

    /// Create a missing field error
    pub fn missing_field<S: Into<String>>(field: S) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    /// Create an invalid field value error
    pub fn invalid_field_value<S: Into<String>>(field: S, value: S) -> Self {
        Self::InvalidFieldValue {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(message: S) -> Self {
        Self::ValidationError {
            message: message.into(),
        }
    }

    /// Create a serialization error
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    /// Create a deserialization error
    pub fn deserialization<S: Into<String>>(message: S) -> Self {
        Self::DeserializationError {
            message: message.into(),
        }
    }

    /// Create a time error
    pub fn time<S: Into<String>>(message: S) -> Self {
        Self::TimeError {
            message: message.into(),
        }
    }

    /// Create a unit conversion error
    pub fn unit_conversion<S: Into<String>>(from: S, to: S) -> Self {
        Self::UnitConversionError {
            from: from.into(),
            to: to.into(),
        }
    }

    /// Create a normalization error
    pub fn normalization<S: Into<String>>(message: S) -> Self {
        Self::NormalizationError {
            message: message.into(),
        }
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for SenMLError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError {
            message: err.to_string(),
        }
    }
}

#[cfg(feature = "cbor")]
impl From<ciborium::de::Error<std::io::Error>> for SenMLError {
    fn from(err: ciborium::de::Error<std::io::Error>) -> Self {
        Self::DeserializationError {
            message: err.to_string(),
        }
    }
}

#[cfg(feature = "cbor")]
impl From<ciborium::ser::Error<std::io::Error>> for SenMLError {
    fn from(err: ciborium::ser::Error<std::io::Error>) -> Self {
        Self::SerializationError {
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = SenMLError::invalid_data("test message");
        assert!(matches!(err, SenMLError::InvalidData { .. }));
        assert_eq!(err.to_string(), "Invalid SenML data: test message");
    }

    #[test]
    fn test_missing_field_error() {
        let err = SenMLError::missing_field("name");
        assert_eq!(err.to_string(), "Missing required field: name");
    }
}