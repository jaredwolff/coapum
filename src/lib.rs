pub mod config;
pub mod credential;
pub mod extract;
pub mod handler;
pub mod helper;
pub mod observer;
pub mod router;
pub mod serve;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

// Re-export commonly used types from the ergonomic API
pub use credential::memory::MemoryCredentialStore;
pub use credential::{ClientInfo, CredentialStore, PskEntry};
pub use extract::state::FullRequest;
pub use extract::{
    Bytes, Cbor, FromRequest, Identity, IntoResponse, Json, ObserveFlag, Path, Raw, Source, State,
    StatusCode,
};
pub use handler::{Handler, HandlerFn, into_handler};
pub use observer::{
    Observer, ObserverChannels, ObserverRequest, ObserverValue, PathValidationError, merge_json,
    path_to_json, validate_observer_path,
};
pub use router::{
    ClientManager, ClientManagerError, ClientMetadata, NotificationTrigger, RouterBuilder,
    StateUpdateError, StateUpdateHandle,
};

// Re-export CoAP types
pub use coap_lite::{
    CoapRequest, CoapResponse, ContentFormat, MessageClass, MessageType, Packet, RequestType,
    ResponseType,
};
pub use webrtc_dtls as dtls;
pub use webrtc_util as util;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;
