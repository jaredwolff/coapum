//! Simple SenML example demonstrating the SenML extractor
//!
//! This example shows basic SenML support:
//! 1. Accept SenML payloads via POST
//! 2. Return SenML responses
//! 3. Demonstrate both JSON and CBOR format support

use coapum::{
    extract::SenML,
    router::RouterBuilder,
    serve,
    observer::memory::MemObserver,
    StatusCode,
};
use coapum_senml::SenMLBuilder;

/// Simple handler that accepts SenML sensor readings and returns an acknowledgment
async fn handle_sensor_data(SenML(pack): SenML) -> SenML {
    println!("Received SenML pack with {} records", pack.len());
    
    // Print each record
    for (i, record) in pack.iter().enumerate() {
        println!("  Record {}: {:?}", i, record);
    }
    
    // Respond with an acknowledgment
    let response = SenMLBuilder::new()
        .base_name("urn:controller1/")
        .add_string_value("status", "received")
        .add_value("record_count", pack.len() as f64)
        .build();
    
    SenML(response)
}

/// Handler that demonstrates creating SenML time-series data  
async fn get_temperature_data() -> SenML {
    let response = SenMLBuilder::new()
        .base_name("urn:sensor1/")
        .base_unit("Cel")
        .base_time(1640995200.0) // Base timestamp
        .add_measurement("temp1", 22.1, 0.0)   // At base time
        .add_measurement("temp1", 22.3, 60.0)  // 1 minute later  
        .add_measurement("temp1", 22.0, 120.0) // 2 minutes later
        .build();
    
    SenML(response)
}

/// Simple ping handler
async fn ping() -> StatusCode {
    println!("Ping received");
    StatusCode::Valid
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    // Simple state - just a unit type
    let app_state = ();
    let observer = MemObserver::new();
    
    let router = RouterBuilder::new(app_state, observer)
        .post("/sensors", handle_sensor_data)
        .get("/temperature", get_temperature_data)  
        .get("/", ping)
        .build();
    
    println!("Starting Simple SenML example server on localhost:5683");
    println!();
    println!("Available endpoints:");
    println!("  POST /sensors     - Send sensor data (SenML format)");
    println!("  GET  /temperature - Get temperature time-series");
    println!("  GET  /            - Ping");
    println!();
    println!("Example SenML JSON payload for POST /sensors:");
    println!(r#"[
  {{"n":"urn:dev:sensor1/", "t": 1640995200}},
  {{"n":"temperature", "v": 22.5, "u": "Cel"}},
  {{"n":"humidity", "v": 45.0, "u": "%RH"}}
]"#);
    
    serve::serve("127.0.0.1:5683".to_string(), Default::default(), router).await?;
    
    Ok(())
}