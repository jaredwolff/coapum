pub mod config;
pub mod extract;
pub mod handler;
pub mod helper;
pub mod observer;
pub mod router;
pub mod serve;

#[cfg(test)]
mod tests;

pub mod test_utils;

// Re-export commonly used types from the ergonomic API
pub use extract::state::FullRequest;
pub use extract::{
    Bytes, Cbor, FromRequest, Identity, IntoResponse, Json, ObserveFlag, Path, Raw, Source, State,
    StatusCode,
};
pub use handler::{into_handler, Handler, HandlerFn};
pub use router::{NotificationTrigger, RouterBuilder, StateUpdateHandle, StateUpdateError};

// Re-export CoAP types
pub use coap_lite::{
    CoapRequest, CoapResponse, ContentFormat, MessageClass, Packet, RequestType, ResponseType,
};
pub use webrtc_dtls as dtls;
pub use webrtc_util as util;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;
