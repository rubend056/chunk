use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chunk::{Chunk, User};
use log::{info, warn};
use serde::{Deserialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};

mod chunk;

#[tokio::main]
async fn main() {
    // Enable env_logger implemenation of log.
    env_logger::init();

    let db = Db::default();

    // Build router
    let app = Router::new()
        .route("/chunk", get(chunk_get).post(chunk_new))
        .layer(Extension(db));

    // Listen
    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

type Db = Arc<RwLock<HashMap<String, Chunk>>>;
async fn chunk_get(Extension(db): Extension<Db>) -> impl IntoResponse {
    let chunks = db.read().unwrap();
    let chunks = chunks.values().cloned().collect::<Vec<_>>();
    Json(chunks)
}

#[derive(Debug, Deserialize)]
struct ChunkIn {
    value: String,
}

// static allow: Regex = Regex::new("[^a-z0-9]").unwrap();
async fn chunk_new(Json(input): Json<ChunkIn>, Extension(db): Extension<Db>) -> impl IntoResponse {
    let chunk = Chunk::new(input.value);
		// let chunk1 = chunk.clone();

    db.write()
        .unwrap()
        .entry(chunk._id.clone())
        .and_modify(|c| {
            c.value = chunk.value.clone();
            c.modified = chunk.modified.clone();
        })
        .or_insert(chunk.clone());

    (StatusCode::CREATED, Json(chunk))
}
