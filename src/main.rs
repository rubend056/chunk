use axum::{
    body::{boxed, Body, BoxBody},
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension, Path,
    },
    http::{header, Request, Response, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use axum_extra::routing::SpaRouter;
use chunk::{Chunk, UTC};
use futures::{
    future::Future,
    future::{select, select_ok, BoxFuture, FusedFuture},
    prelude::*,
    select,
    stream::FuturesUnordered,
};

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env::{self, VarError},
    fs,
    net::{SocketAddr, SocketAddrV4},
    str::FromStr,
    sync::{Arc, RwLock},
};
use tokio::signal::{
    unix,
    unix::{signal, Signal, SignalKind},
};
use tower::{util::ServiceExt, Layer};
use tower_http::{
    cors::CorsLayer,
    services::{Redirect, ServeDir},
    trace::TraceLayer,
};

mod chunk;

#[tokio::main]
async fn main() {
    // Enable env_logger implemenation of log.
    env_logger::init();

    let db_path = env::var("DB_PATH").unwrap_or("db.json".to_string());

    let db = match env::var("DB_INIT") {
        Ok(db_init) => {
            info!("Fetching {db_init}");
            let db = serde_json::from_slice::<Vec<Chunk>>(
                &reqwest::get(&db_init).await.unwrap().bytes().await.unwrap(),
            )
            .unwrap();

            let hm = db
                .iter()
                .map(|c| (c.created.clone(), c.clone()))
                .collect::<HashMap<UTC, Chunk>>();
            info!("Fetched {db_init} for {} chunks", hm.len());
            CreatedToChunk::from(RwLock::from(hm))
        }
        _ => match fs::read_to_string(&db_path) {
            Ok(db) => {
                let hm = serde_json::from_str::<HashMap<UTC, Chunk>>(&db)
                    .expect(&format!("Couldn't read {db_path}"));
                info!("Read {db_path} for {} chunks", hm.len());
                CreatedToChunk::from(RwLock::from(hm))
            }
            _ => {
                info!("Reading {db_path} failed, initializing with 0 chunks");
                CreatedToChunk::default()
            }
        },
    };

    let web_dist = env::var("WEB_DIST").unwrap_or("web".to_string());
    info!("WEB_DIST=\"{}\"", web_dist);

    // let (tx_change, rx_change) = sync::broadcast::channel(5);

    // Build router
    let app = Router::new()
        .route(
            "/chunks",
            get(chunks_get).put(chunks_put).delete(chunks_del),
        )
        .route("/", get(root))
        .merge(SpaRouter::new("/web", web_dist))
        .layer(Extension(db.clone()))
        .layer(
            tower::ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        );

    // Spawn a task to gracefully shutdown server.
    // let (tx_shutdown, rx_shutdown) = sync::broadcast::channel::<()>(5);
    // tokio::spawn(shutdown_handler(tx_shutdown, rx_change));

    // Create Socket to listen on
    let addr = SocketAddrV4::from_str(
        format!("0.0.0.0:{}", env::var("PORT").unwrap_or("4000".to_string())).as_str(),
    )
    .expect("Parsing address shouldn't fail");
    info!("listening on {}", addr);

    // Create server
    let server = axum::Server::bind(&addr.into())
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            let si = signal(SignalKind::interrupt()).unwrap();
            let st = signal(SignalKind::terminate()).unwrap();
            let mut s_arr = [si, st];
            let unordered = FuturesUnordered::from_iter(s_arr.iter_mut().map(|f| f.recv().fuse()));
            unordered.take(1).collect::<Vec<_>>().await;
            info!("Shutting down.");
        });

    // Start server by waiting on it's Future
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }

    // Saving to json
    let db_read = db.read().unwrap();
    let data = serde_json::to_string_pretty(&db_read.clone()).unwrap();
    match fs::write(&db_path, &data) {
        Ok(()) => info!("Saved {} chunks on {db_path}", db_read.len()),
        Err(e) => {
						error!("Error saving to path {db_path}: {e}");
						let backup_path = "chunks.backup.json".to_string();
            match fs::write(&backup_path, &data) {
                Ok(()) => info!("Saved {} chunks on {backup_path}", db_read.len()),
                Err(e) => error!("Error saving to backup path {backup_path}: {e}"),
            }
        }
    };
}

async fn root() -> impl IntoResponse {
    ([(header::LOCATION, "/web")], StatusCode::MOVED_PERMANENTLY)
}

type CreatedToChunk = Arc<RwLock<HashMap<UTC, Chunk>>>;
type TitleToCreated = Arc<RwLock<HashMap<String, UTC>>>;
async fn chunks_get(Extension(db): Extension<CreatedToChunk>) -> impl IntoResponse {
    let chunks = db.read().unwrap();
    let mut chunks = chunks.values().cloned().collect::<Vec<_>>();
    chunks.sort_by_key(|c| c.created);
    info!("GET /chunks len {}", chunks.len());
    Json(chunks)
}

#[derive(Debug, Deserialize)]
struct ChunkIn {
    value: String,
    created: Option<UTC>,
}

async fn chunks_put(
    Json(input): Json<ChunkIn>,
    Extension(db): Extension<CreatedToChunk>,
) -> impl IntoResponse {
    let chunk = Chunk::new(&input.value);
    let mut db = db.write().unwrap();

    match chunk {
        Ok(mut chunk) => {
            let mut modified = false;
            if let Some(created) = input.created {
                if db.contains_key(&created) {
                    chunk.created = created;
                    modified = true;
                }
            }
            db.insert(chunk.created.clone(), chunk.clone());
            info!(
                "PUT /chunks {}: {chunk:?}",
                if modified { "modified" } else { "created" }
            );
            Ok((
                if modified {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                Json(chunk),
            ))
        }
        Err(err) => {
            info!("PUT /chunks error, input: {input:?}");
            Err((
                StatusCode::NOT_ACCEPTABLE,
                format!("Chunk couldn't be created {}", err),
            ))
        }
    }
}

async fn chunks_del(
    Json(input): Json<Vec<Chunk>>,
    Extension(db): Extension<CreatedToChunk>,
) -> impl IntoResponse {
    let mut db = db.write().unwrap();

    for chunk in &input {
        if !db.contains_key(&chunk.created) {
            return Err((StatusCode::NOT_FOUND, format!("{chunk:?}")));
        }
    }
    for chunk in &input {
        db.remove(&chunk.created);
    }

    info!("DELETE /chunks {}", input.len());

    Ok(StatusCode::OK)
}

// Websocket stream

// struct State {
// 	clients_count: usize,
// 	rx: watch::Receiver<Message>,
// }

// #[derive(Serialize)]
// struct WebsocketStatus {
// 	clients_count: usize,
// 	is_up: bool,
// }

// async fn stream_handler(
// 	ws: WebSocketUpgrade,
// 	Extension(state): Extension<Arc<Mutex<State>>>,
// ) -> impl IntoResponse {
// 	ws.on_upgrade(|socket| stream(socket, state))
// }

// async fn stream(stream: WebSocket, state: Arc<Mutex<State>>) {
// 	// By splitting we can send and receive at the same time.
// 	let (mut sender, mut receiver) = stream.split();

// 	let mut rx = {
// 			let mut state = state.lock().await;
// 			state.clients_count += 1;
// 			state.rx.clone()
// 	};

// 	// This task will receive watch messages and forward it to this connected client.
// 	let mut send_task = tokio::spawn(async move {
// 			while let Ok(()) = rx.changed().await {
// 					let msg = rx.borrow().clone();

// 					if sender.send(msg).await.is_err() {
// 							break;
// 					}
// 			}
// 	});

// 	// This task will receive messages from this client.
// 	let mut recv_task = tokio::spawn(async move {
// 			while let Some(Ok(Message::Text(text))) = receiver.next().await {
// 					println!("this example does not read any messages, but got: {text}");
// 			}
// 	});

// 	// If any one of the tasks exit, abort the other.
// 	tokio::select! {
// 			_ = (&mut send_task) => recv_task.abort(),
// 			_ = (&mut recv_task) => send_task.abort(),
// 	};

// 	// This client disconnected
// 	state.lock().await.clients_count -= 1;
// }
