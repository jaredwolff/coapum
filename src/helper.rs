use std::io::Cursor;

use ciborium::value::Value as CborValue;
use serde_json::{map::Entry, Value as JsonValue};

pub fn convert_cbor_to_json(cbor_data: &[u8]) -> serde_json::Result<JsonValue> {
    let cbor_value: CborValue = ciborium::de::from_reader(Cursor::new(cbor_data)).unwrap();
    let json_value: JsonValue = serde_json::to_value(cbor_value)?;
    Ok(json_value)
}

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

pub fn json_merge(a: &mut JsonValue, b: &JsonValue) {
    match (a, b) {
        (&mut JsonValue::Object(ref mut a), JsonValue::Object(ref b)) => {
            for (k, v) in b {
                match a.entry(k.clone()) {
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                    Entry::Occupied(mut e) => json_merge(e.get_mut(), v),
                }
            }
        }
        (a, b) => *a = b.clone(),
    }
}
