use std::io::Cursor;

use ciborium::value::Value as CborValue;
use serde_json::Value as JsonValue;

/// Converts CBOR data to JSON format.
///
/// This function accepts a byte slice (`&[u8]`) representing CBOR data,
/// and converts it into a `serde_json::Value` representing the equivalent JSON data.
/// If the conversion is successful, it returns `Ok(JsonValue)`. If it fails, it returns an
/// error of type `serde_json::Error`.
///
/// The conversion process is as follows:
/// 1. The CBOR data is deserialized into a `ciborium::value::Value` (aliased as `CborValue`).
/// 2. The `CborValue` is then converted into a `serde_json::Value` (aliased as `JsonValue`).
///
/// # Arguments
///
/// * `cbor_data` - A byte slice that encodes CBOR data.
///
/// # Returns
///
/// * `serde_json::Result<JsonValue>` - If successful, the function returns `Ok(JsonValue)`,
/// where `JsonValue` is the equivalent JSON data. If the conversion fails, it returns an
/// error of type `serde_json::Error`.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use coapum::helper::convert_cbor_to_json;
/// 
/// let cbor_data: Vec<u8> = vec![0xA1, 0x63, 0x66, 0x6F, 0x6F, 0x63, 0x62, 0x61, 0x72]; // Equivalent to {"foo": "bar"}
/// let json_value = convert_cbor_to_json(&cbor_data).unwrap();
/// assert_eq!(json_value, json!({"foo": "bar"}));
/// ```
pub fn convert_cbor_to_json(cbor_data: &[u8]) -> serde_json::Result<JsonValue> {
    let cbor_value: CborValue = ciborium::de::from_reader(Cursor::new(cbor_data)).unwrap();
    let json_value: JsonValue = serde_json::to_value(cbor_value)?;
    Ok(json_value)
}

/// Converts a JSON string to CBOR format.
///
/// This function accepts a string slice (`&str`) representing JSON data,
/// and converts it into a byte vector (`Vec<u8>`) representing the equivalent CBOR data.
/// If the conversion is successful, it returns `Ok(Vec<u8>)`. If it fails, it returns an
/// error of type `Box<dyn std::error::Error>`.
///
/// The conversion process is as follows:
/// 1. The JSON string is parsed into a `serde_json::Value` (aliased as `JsonValue`).
/// 2. The `JsonValue` is then converted into a `ciborium::value::Value` (aliased as `CborValue`).
/// 3. The `CborValue` is serialized into CBOR format and written into a buffer.
///
/// # Arguments
///
/// * `json` - A string slice that encodes JSON data.
///
/// # Returns
///
/// * `Result<Vec<u8>, Box<dyn std::error::Error>>` - If successful, the function returns `Ok(Vec<u8>)`,
/// where `Vec<u8>` is the equivalent CBOR data. If the conversion fails, it returns an
/// error of type `Box<dyn std::error::Error>`.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use coapum::helper::convert_json_to_cbor;
/// 
/// let json_string = json!({"foo": "bar"}).to_string();
/// let cbor_data = convert_json_to_cbor(&json_string).unwrap();
/// assert_eq!(cbor_data, vec![0xA1, 0x63, 0x66, 0x6F, 0x6F, 0x63, 0x62, 0x61, 0x72]); // Equivalent to {"foo": "bar"}
/// ```
pub fn convert_json_to_cbor(json: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Parse the JSON string into a serde_json::Value
    let json_value: JsonValue = serde_json::from_str(json)?;

    // Convert the JSON value to a CBOR value
    let cbor_value: CborValue = serde_json::from_value(json_value)?;

    // Create a buffer to hold the serialized CBOR
    let mut buffer = Vec::new();

    // Serialize the CBOR value into the buffer
    ciborium::ser::into_writer(&cbor_value, &mut buffer)?;

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_json_to_cbor() {
        let json = r#"{"age": 30}"#;
        let expected_cbor = vec![
            0xa1, // map(2)
            0x63, 0x61, 0x67, 0x65, // text(4): "age"
            0x18, 0x1e, // unsigned(30)
        ];

        let result = convert_json_to_cbor(json).unwrap();
        assert_eq!(result, expected_cbor);
    }
}
