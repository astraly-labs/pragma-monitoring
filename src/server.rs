use axum::{routing::get, Router};
use hyper::Server;
use prometheus::{Encoder, TextEncoder};
use std::net::SocketAddr;

pub async fn run_metrics_server() {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/metrics", get(metrics_handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    println!("Listening on http://{}", addr);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn root_handler() -> String {
    "<a href=\"/metrics\">/metrics</a>".to_string()
}

async fn metrics_handler() -> hyper::Response<hyper::Body> {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    encoder.encode(&metric_families, &mut buffer).unwrap();

    hyper::Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(hyper::Body::from(buffer))
        .unwrap()
}
