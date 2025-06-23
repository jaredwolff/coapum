use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use coapum::{
    router::{CoapumRequest, RouterBuilder},
    Raw, {CoapRequest, Packet},
};

use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use tower::Service; // make sure to use your actual project name and import path

async fn test() -> Raw {
    let json = "{\"resp\":\"OK\"}";
    log::info!("Writing: {}", json);

    Raw {
        payload: json.as_bytes().to_vec(),
        content_format: None,
    }
}

fn router_benchmark(c: &mut Criterion) {
    let mut router = RouterBuilder::new((), ()).get("test", test).build();

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
            let _ = router.call(std::hint::black_box::<CoapumRequest<SocketAddr>>(request));
        })
    });
}

criterion_group!(benches, router_benchmark);
criterion_main!(benches);
