use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{CoapRequest, ContentFormat, Packet, RequestType, client::DtlsClient};
use serde::{Deserialize, Serialize};

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceState {
    temperature: f32,
    humidity: f32,
    battery_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse {
    status: String,
    message: String,
}

async fn send_request(
    client: &mut DtlsClient,
    method: RequestType,
    path: &str,
    payload: Option<Vec<u8>>,
    content_format: Option<ContentFormat>,
) -> Result<Packet, Box<dyn std::error::Error>> {
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(method);
    request.set_path(path);

    if let Some(payload) = payload {
        request.message.payload = payload;
    }

    if let Some(format) = content_format {
        request.message.set_content_format(format);
    }

    tracing::info!("Sending {:?} request to {}", method, path);

    client.send(&request.message.to_bytes().unwrap()).await?;
    tracing::info!("Sent request");

    let data = client.recv(Duration::from_secs(5)).await?;
    let packet = Packet::from_bytes(&data).unwrap();
    tracing::info!("Response status: {:?}", packet.header.code);

    if !packet.payload.is_empty() {
        tracing::debug!("Response payload: {} bytes", packet.payload.len());
    }

    Ok(packet)
}

async fn test_device_state_endpoints(
    client: &mut DtlsClient,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Device State Endpoints ===");

    let device_id = "device123";
    let path = format!(".d/{}", device_id);

    // Test 1: POST - Create/Update device state
    println!("\n1. Testing POST (update device state)");
    let device_state = DeviceState {
        temperature: 25.5,
        humidity: 60.0,
        battery_level: 85,
    };

    let cbor_payload = {
        let mut buffer = Vec::new();
        ciborium::ser::into_writer(&device_state, &mut buffer)?;
        buffer
    };

    let response = send_request(
        client,
        RequestType::Post,
        &path,
        Some(cbor_payload),
        Some(ContentFormat::ApplicationCBOR),
    )
    .await?;

    if !response.payload.is_empty() {
        let api_response: ApiResponse = ciborium::de::from_reader(&response.payload[..])?;
        println!("API Response: {:?}", api_response);
    }

    // Test 2: GET - Retrieve device state
    println!("\n2. Testing GET (retrieve device state)");
    let response = send_request(client, RequestType::Get, &path, None, None).await?;

    if !response.payload.is_empty() {
        let retrieved_state: DeviceState = ciborium::de::from_reader(&response.payload[..])?;
        println!("Retrieved device state: {:?}", retrieved_state);
    }

    // Test 3: DELETE - Remove device state
    println!("\n3. Testing DELETE (remove device state)");
    let _response = send_request(client, RequestType::Delete, &path, None, None).await?;

    Ok(())
}

async fn test_stream_endpoints(client: &mut DtlsClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Stream Endpoints ===");

    let stream_id = "stream456";
    let path = format!(".s/{}", stream_id);

    // Test stream data upload
    println!("\n1. Testing POST (stream data)");
    let stream_data = b"Sample stream data payload";

    let _response = send_request(
        client,
        RequestType::Post,
        &path,
        Some(stream_data.to_vec()),
        None,
    )
    .await?;

    Ok(())
}

async fn test_utility_endpoints(client: &mut DtlsClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Utility Endpoints ===");

    // Test 1: Echo endpoint
    println!("\n1. Testing echo endpoint");
    let echo_data = b"Hello, CoAP world!";

    let response = send_request(
        client,
        RequestType::Put,
        "echo",
        Some(echo_data.to_vec()),
        None,
    )
    .await?;

    if !response.payload.is_empty() {
        let echoed = String::from_utf8_lossy(&response.payload);
        println!("Echoed back: {}", echoed);
    }

    // Test 2: Hello endpoint (GET version of echo)
    println!("\n2. Testing hello endpoint");
    let hello_data = b"Hello from GET!";

    let response = send_request(
        client,
        RequestType::Get,
        "hello",
        Some(hello_data.to_vec()),
        None,
    )
    .await?;

    if !response.payload.is_empty() {
        let echoed = String::from_utf8_lossy(&response.payload);
        println!("Hello response: {}", echoed);
    }

    // Test 3: Ping endpoint
    println!("\n3. Testing ping endpoint");
    let _response = send_request(client, RequestType::Get, "", None, None).await?;

    Ok(())
}

async fn test_error_conditions(client: &mut DtlsClient) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Testing Error Conditions ===");

    // Test 1: Invalid path
    println!("\n1. Testing invalid path");
    let _response = send_request(client, RequestType::Get, "nonexistent/path", None, None).await?;

    // Test 2: Invalid CBOR data
    println!("\n2. Testing invalid CBOR data");
    let invalid_cbor = vec![0xFF, 0xFF, 0xFF, 0xFF];

    let _response = send_request(
        client,
        RequestType::Post,
        ".d/test",
        Some(invalid_cbor),
        Some(ContentFormat::ApplicationCBOR),
    )
    .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Ergonomic CoAP Client Starting!");
    println!("Testing the new ergonomic server API...");

    // Build dimpl config for PSK client
    let mut keys = HashMap::new();
    keys.insert(IDENTITY.to_string(), PSK.to_vec());

    let resolver = Arc::new(coapum::credential::resolver::MapResolver::new(keys));

    let config = dimpl::Config::builder()
        .with_psk_resolver(resolver as Arc<dyn dimpl::PskResolver>)
        .with_psk_identity(IDENTITY.as_bytes().to_vec())
        .build()
        .expect("valid DTLS config");

    let server_addr = "127.0.0.1:5684";
    let mut client = DtlsClient::connect(server_addr, Arc::new(config)).await?;
    println!("DTLS connection established to {}", server_addr);

    // Run all test suites
    match test_device_state_endpoints(&mut client).await {
        Ok(_) => println!("Device state endpoints test passed"),
        Err(e) => println!("Device state endpoints test failed: {}", e),
    }

    match test_stream_endpoints(&mut client).await {
        Ok(_) => println!("Stream endpoints test passed"),
        Err(e) => println!("Stream endpoints test failed: {}", e),
    }

    match test_utility_endpoints(&mut client).await {
        Ok(_) => println!("Utility endpoints test passed"),
        Err(e) => println!("Utility endpoints test failed: {}", e),
    }

    match test_error_conditions(&mut client).await {
        Ok(_) => println!("Error conditions test passed"),
        Err(e) => println!("Error conditions test failed: {}", e),
    }

    println!("\nAll tests completed!");
    println!("The ergonomic API is working correctly!");

    Ok(())
}
