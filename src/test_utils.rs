//! Test utilities for creating test requests and responses
//!
//! This module provides helper functions for creating test requests
//! that can be used across different test modules.

use crate::router::CoapumRequest;
use crate::{CoapRequest, Packet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// Create a test request for the given path
pub fn create_test_request(path: &str) -> CoapumRequest<SocketAddr> {
    let mut request = CoapRequest::from_packet(
        Packet::new(),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
    );
    request.set_path(path);
    request.into()
}

/// Create a test POST request with custom payload
pub fn create_test_request_with_payload(path: &str, payload: Vec<u8>) -> CoapumRequest<SocketAddr> {
    let mut request = CoapRequest::from_packet(
        Packet::new(),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
    );
    request.set_path(path);
    request.set_method(crate::RequestType::Post);
    request.message.payload = payload;
    request.into()
}

/// Create a test POST request with payload and content format
pub fn create_test_request_with_content(
    path: &str, 
    payload: Vec<u8>, 
    content_format: crate::ContentFormat
) -> CoapumRequest<SocketAddr> {
    let mut request = CoapRequest::from_packet(
        Packet::new(),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
    );
    request.set_path(path);
    request.set_method(crate::RequestType::Post);
    request.message.payload = payload;
    request.message.set_content_format(content_format);
    request.into()
}