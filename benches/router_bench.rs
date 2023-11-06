use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use coapum::{
    router::{wrapper::get, CoapRouter, CoapumRequest, Request, RouterError},
    {CoapRequest, CoapResponse, Packet, ResponseType},
};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::json;
use tower::Service; // make sure to use your actual project name and import path

async fn test<S>(payload: Box<dyn Request>, _state: S) -> CoapResponse {
    log::info!("Got json payload: {}", payload.get_value());

    let pkt = Packet::default();
    let mut response = CoapResponse::new(&pkt).unwrap();
    let json = "{\"resp\":\"OK\"}";
    response.message.payload = json.as_bytes().to_vec();
    response.set_status(ResponseType::Valid);

    log::info!("Writing: {}", json);
    response
}

fn router_benchmark(c: &mut Criterion) {
    let mut router = CoapRouter::new((), ());
    router.add("test", get(test));

    c.bench_function("coap_router", |b| {
        b.iter(|| {
            let mut pkt = Packet::new();

            let value = json!({
                "code": 415,
                "message": null,
                "continue": false,
                "extra": { "numbers" : [8.2341e+4, 0.251425] },
            });

            // Serialize the value into the buffer
            let buffer = serde_json::to_vec(&value).unwrap();

            // Set value
            pkt.payload = buffer;

            let request = CoapRequest::from_packet(
                pkt,
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
            );

            let request: CoapumRequest<SocketAddr> = request.into();
            router.call(black_box::<CoapumRequest<SocketAddr>>(request));
        })
    });
}

criterion_group!(benches, router_benchmark);
criterion_main!(benches);
