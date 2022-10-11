use axum::{
    error_handling::HandleError,
    extract::Extension,
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Error, Json, Router,
};
use chunk::{Chunk, User, UTC};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tokio::signal;
use tower_http::cors::{Any, CorsLayer};

mod chunk;

#[tokio::main]
async fn main() {
    // Enable env_logger implemenation of log.
    env_logger::init();

    let db = match fs::read_to_string("db.json") {
        Ok(str) => CreatedToChunk::from(RwLock::from(
            serde_json::from_str::<HashMap<UTC, Chunk>>(&str).expect("Couldn't read db.json"),
        )),
        Err(err) => CreatedToChunk::default(),
    };

    // Build router
    let app = Router::new()
        .route(
            "/chunks",
            get(chunks_get).put(chunks_put).delete(chunks_del),
        )
        .layer(Extension(db))
        .layer(CorsLayer::permissive());

    // match signal::ctrl_c().await {
    //     Ok(()) => {
    //         info!("Shutting down...")
    //     }
    //     Err(err) => {
    //         eprintln!("Unable to listen for shutdown signal: {}", err);
    //         // we also shut down in case of error
    //     }
    // }
    // Listen
    let addr = SocketAddr::from(([127, 0, 0, 1], 4000));
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

type CreatedToChunk = Arc<RwLock<HashMap<UTC, Chunk>>>;
type TitleToCreated = Arc<RwLock<HashMap<String, UTC>>>;
async fn chunks_get(Extension(db): Extension<CreatedToChunk>) -> impl IntoResponse {
    let chunks = db.read().unwrap();
    let mut chunks = chunks.values().cloned().collect::<Vec<_>>();
    chunks.sort_by_key(|c| c.created);
    Json(chunks)
}

#[derive(Debug, Deserialize)]
struct ChunkIn {
    value: String,
    created: Option<UTC>,
}

// static allow: Regex = Regex::new("[^a-z0-9]").unwrap();
async fn chunks_put(
    Json(input): Json<ChunkIn>,
    Extension(db): Extension<CreatedToChunk>,
) -> impl IntoResponse {
    let chunk = Chunk::new(&input.value);
    let mut db = db.write().unwrap();

    match chunk {
        Ok(mut chunk) => {
            if let Some(created) = input.created {
                if db.contains_key(&created) {
                    chunk.created = created;
                }
            }
            db.insert(chunk.created.clone(), chunk);

            (StatusCode::CREATED, format!("Chunk created"))
        }
        Err(err) => (
            StatusCode::NOT_ACCEPTABLE,
            format!("Chunk couldn't be created {}", err),
        ),
    }
}

async fn chunks_del(
    Json(input): Json<Vec<Chunk>>,
    Extension(db): Extension<CreatedToChunk>,
) -> impl IntoResponse {
    let mut db = db.write().unwrap();

    // let chunks = ;
    for chunk in &input {
        if !db.contains_key(&chunk.created) {
            return (StatusCode::NOT_FOUND, format!("Not found {chunk:?}"));
        }
    }
    for chunk in &input {
        db.remove(&chunk.created);
    }

    (StatusCode::OK, format!(""))
}
