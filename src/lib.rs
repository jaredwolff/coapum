pub mod client;
pub mod config;
pub mod credential;
mod error;
pub mod extract;
pub mod handler;
pub mod helper;
pub mod middleware;
pub mod observer;
pub mod reliability;
pub mod router;
pub mod serve;
pub mod service;

pub use error::Error;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

// Re-export commonly used types from the ergonomic API
pub use credential::memory::MemoryCredentialStore;
pub use credential::{ClientInfo, CredentialStore, PskEntry};
pub use extract::state::FullRequest;
pub use extract::{
    Block2Request, BlockedRaw, Bytes, Cbor, FromRequest, Identity, IntoResponse, Json, ObserveFlag,
    Path, Raw, Source, State, StatusCode,
};
pub use handler::{Handler, HandlerFn, into_handler};
pub use observer::{
    Observer, ObserverChannels, ObserverRequest, ObserverValue, PathValidationError, cbor_pointer,
    merge_cbor, path_to_cbor, validate_observer_path,
};
pub use router::layered::{
    LayeredCoapRouter, LayeredCoapRouterNotificationOnly, LayeredCoapRouterRequestOnly,
};
pub use router::{
    BlockTransferEvent, ClientManager, ClientManagerError, ClientMetadata, CoapumRequest,
    DeviceEvent, NotificationTrigger, RouterBuilder, StateUpdateError, StateUpdateHandle,
};
pub use serve::{ServerHandle, SessionHandle, SessionId, bind_and_spawn};
pub use service::CoapService;

// Re-export CoAP types
pub use coap_lite::{
    CoapOption, CoapRequest, CoapResponse, ContentFormat, MessageClass, MessageType, ObserveOption,
    Packet, RequestType, ResponseType, block_handler::BlockValue,
};
pub use dimpl as dtls;

#[cfg(test)]
#[macro_use]
extern crate lazy_static;
