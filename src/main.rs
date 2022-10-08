use axum::{
    extract::Extension,
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
		error_handling::HandleError,
		Error,
};
use chunk::{Chunk, User};
use log::{info, warn};
use serde::Deserialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tower_http::cors::{Any, CorsLayer};

mod chunk;

#[tokio::main]
async fn main() {
    // Enable env_logger implemenation of log.
    env_logger::init();

    let db = Db::default();

    // let cors = CorsLayer::new()
    //     // allow `GET` and `POST` when accessing the resource
    //     .allow_methods([Method::GET, Method::POST])
    //     // allow requests from any origin
    //     .allow_origin(Any);

    // Build router
    let app = Router::new()
        .route("/chunks", get(chunks_get).put(chunks_new))
        .layer(Extension(db))
        .layer(CorsLayer::permissive());
				
		

    // Listen
    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}


type Db = Arc<RwLock<HashMap<String, Chunk>>>;
async fn chunks_get(Extension(db): Extension<Db>) -> impl IntoResponse {
    let chunks = db.read().unwrap();
    let chunks = chunks.values().cloned().collect::<Vec<_>>();
    Json(chunks)
}

#[derive(Debug, Deserialize)]
struct ChunkIn {
    value: String,
}

// static allow: Regex = Regex::new("[^a-z0-9]").unwrap();
async fn chunks_new(Json(input): Json<ChunkIn>, Extension(db): Extension<Db>) -> impl IntoResponse {
    match Chunk::new(input.value) {
        Ok(chunk) => {
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
        Err(err) => panic!("Damn, this sucks"),
    }
}
