//! # CoAPum SenML - Sensor Measurement Lists for Rust
//!
//! A Rust implementation of [RFC 8428](https://tools.ietf.org/html/rfc8428) - Sensor Measurement Lists (SenML).
//!
//! SenML is a format for representing simple sensor measurements and device parameters
//! in a structured way. This crate provides type-safe handling of SenML data with
//! support for multiple serialization formats including JSON, CBOR, and XML.
//!
//! ## Features
//!
//! - **RFC 8428 Compliant**: Full support for SenML specification
//! - **Multiple Formats**: JSON, CBOR, XML serialization support
//! - **Type Safety**: Strongly typed sensor data with validation
//! - **Normalization**: Convert SenML packs to resolved form
//! - **Builder Pattern**: Ergonomic API for creating SenML data
//! - **Time Series**: Specialized support for time-series sensor data
//!
//! ## Quick Start
//!
//! ```rust
//! use coapum_senml::{SenMLPack, SenMLRecord, SenMLBuilder, Result};
//!
//! fn example() -> Result<()> {
//!     // Create a simple temperature reading
//!     let pack = SenMLBuilder::new()
//!         .base_name("urn:dev:sensor1")
//!         .base_unit("Cel")
//!         .add_value("temperature", 22.5)
//!         .build();
//!
//!     // Serialize to JSON
//!     let json = pack.to_json()?;
//!     println!("{}", json);
//!     Ok(())
//! }
//! ```
//!
//! ## SenML Data Model
//!
//! SenML represents sensor data as an array of records, where each record can contain:
//! - **Base fields**: Apply to multiple records (bn, bt, bu, bv, bs, bver)  
//! - **Record fields**: Individual measurements (n, u, v, vs, vb, vd, s, t, ut)
//!
//! Base fields reduce redundancy by providing default values that apply to
//! subsequent records in the pack.

pub mod builder;
pub mod error;
pub mod normalize;
pub mod pack;
pub mod record;

#[cfg(feature = "validation")]
pub mod validation;

#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "cbor")]
pub mod cbor;

#[cfg(feature = "xml")]
pub mod xml;

// Re-export main types
pub use builder::SenMLBuilder;
pub use error::{Result, SenMLError};
pub use normalize::{NormalizedPack, NormalizedRecord};
pub use pack::SenMLPack;
pub use record::{SenMLRecord, SenMLValue};

#[cfg(feature = "validation")]
pub use validation::Validate;

/// SenML Content-Format identifiers for CoAP
pub mod content_format {
    /// application/senml+json
    pub const SENML_JSON: u16 = 110;
    /// application/sensml+json  
    pub const SENSML_JSON: u16 = 111;
    /// application/senml+cbor
    pub const SENML_CBOR: u16 = 112;
    /// application/sensml+cbor
    pub const SENSML_CBOR: u16 = 113;
    /// application/senml-exi
    pub const SENML_EXI: u16 = 114;
    /// application/sensml-exi
    pub const SENSML_EXI: u16 = 115;
    /// application/senml+xml
    pub const SENML_XML: u16 = 310;
    /// application/sensml+xml
    pub const SENSML_XML: u16 = 311;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_senml_creation() {
        let pack = SenMLBuilder::new()
            .base_name("urn:dev:sensor1")
            .add_value("temperature", 22.5)
            .build();

        assert!(pack.records.len() == 2); // Base record + measurement record
        // Check that base name was stored in the first record
        assert_eq!(pack.records[0].n, Some("urn:dev:sensor1".to_string()));
        // Check that measurement record exists
        assert_eq!(pack.records[1].v, Some(22.5));
    }
}
