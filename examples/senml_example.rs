//! Example demonstrating SenML support in CoAPum
//!
//! This example shows how to:
//! 1. Create CoAP handlers that accept SenML data
//! 2. Automatically deserialize SenML JSON and CBOR payloads
//! 3. Respond with SenML data
//! 4. Use various SenML builders for different scenarios

use coapum::{
    extract::{SenML, State},
    router::RouterBuilder,
    serve,
    observer::memory::MemObserver,
};
use coapum_senml::{SenMLBuilder, SenMLPack};
use tokio;

/// Simple application state to store sensor data
#[derive(Debug, Default, Clone)]
struct AppState {
    latest_readings: std::sync::Arc<tokio::sync::Mutex<Vec<SenMLPack>>>,
}

impl AsRef<AppState> for AppState {
    fn as_ref(&self) -> &AppState {
        self
    }
}

/// Handler that accepts SenML sensor readings
async fn handle_sensor_data(
    SenML(pack): SenML, 
    State(state): State<AppState>
) -> SenML {
    println!("Received SenML pack with {} records", pack.len());
    
    // Normalize the pack for easier processing
    let normalized = pack.normalize();
    
    for record in &normalized.records {
        println!("  Record: {} = {:?} {} at {:?}", 
                record.name, 
                record.value.or_else(|| record.string_value.as_ref().and_then(|s| s.parse::<f64>().ok())),
                record.unit.as_deref().unwrap_or(""),
                record.time);
    }
    
    // Store the readings
    let mut readings = state.latest_readings.lock().await;
    readings.push(pack);
    let count = readings.len();
    drop(readings); // Release the lock
    
    // Respond with an acknowledgment
    let response = SenMLBuilder::new()
        .base_name("urn:controller1/")
        .add_string_value("status", "received")
        .add_value("count", count as f64)
        .build();
    
    SenML(response)
}

/// Handler that returns the latest sensor readings in SenML format
async fn get_latest_readings(
    State(state): State<AppState>,
) -> SenML {
    let readings = state.latest_readings.lock().await;
    
    if readings.is_empty() {
        // Return empty readings pack
        let pack = SenMLBuilder::new()
            .base_name("urn:controller1/readings/")
            .add_string_value("message", "no data available")
            .build();
        return SenML(pack);
    }
    
    // Combine all readings into a single normalized response
    let mut all_records = Vec::new();
    
    for pack in readings.iter() {
        let normalized = pack.normalize();
        all_records.extend(normalized.records);
    }
    
    // Create a response pack with all readings
    let mut response_builder = SenMLBuilder::new()
        .base_name("urn:controller1/readings/")
        .base_time(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64()
        );
    
    // Add recent readings (last 10)
    let recent_count = std::cmp::min(10, all_records.len());
    for record in &all_records[all_records.len() - recent_count..] {
        let relative_name = record.local_name();
        
        if let Some(value) = record.value {
            response_builder = response_builder.add_value(relative_name, value);
        } else if let Some(ref string_val) = record.string_value {
            response_builder = response_builder.add_string_value(relative_name, string_val);
        } else if let Some(bool_val) = record.bool_value {
            response_builder = response_builder.add_bool_value(relative_name, bool_val);
        }
    }
    
    SenML(response_builder.build())
}

/// Handler that demonstrates time-series SenML data
async fn get_temperature_history() -> SenML {
    use coapum_senml::builder::TimeSeriesBuilder;
    
    let base_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64() - 300.0; // 5 minutes ago
    
    let pack = TimeSeriesBuilder::new("urn:sensor1/temperature", base_time)
        .unit("Cel")
        .measurement(0.0, 22.1)    // 5 minutes ago
        .measurement(60.0, 22.3)   // 4 minutes ago
        .measurement(120.0, 22.0)  // 3 minutes ago
        .measurement(180.0, 22.5)  // 2 minutes ago
        .measurement(240.0, 22.7)  // 1 minute ago
        .measurement(300.0, 22.4)  // now
        .build();
    
    SenML(pack)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let app_state = AppState::default();
    let observer = MemObserver::new();
    
    let router = RouterBuilder::new(app_state, observer)
        .get("/readings", get_latest_readings)
        .post("/sensors", handle_sensor_data)
        .get("/temperature/history", get_temperature_history)
        .build();
    
    println!("Starting SenML example server on localhost:5683");
    println!();
    println!("Try these requests:");
    println!("  GET /readings - Get latest sensor readings");
    println!("  GET /temperature/history - Get temperature time-series");
    println!("  POST /sensors - Send sensor data (SenML format)");
    println!();
    println!("Example SenML JSON payload for POST /sensors:");
    println!(r#"[
  {{"n":"urn:dev:sensor1/", "t": 1234567890}},
  {{"n":"temperature", "v": 22.5, "u": "Cel"}},
  {{"n":"humidity", "v": 45.0, "u": "%RH"}}
]"#);
    
    serve::serve("127.0.0.1:5683".to_string(), Default::default(), router).await?;
    
    Ok(())
}