use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use std::env;

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "kurye-takip-railway"}))
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let app = Router::new().route("/health", get(health));
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
