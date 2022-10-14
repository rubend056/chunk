use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
     Router, Extension,
};
use axum_extra::routing::SpaRouter;
use futures::{
    prelude::*,
    stream::FuturesUnordered,
};

use log::{info};
use std::{
    net::{SocketAddr, },
    str::FromStr,
    sync::{Arc, RwLock},
};
use tokio::signal::{
  
    unix::{signal, SignalKind},
};
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
};

mod chunk;
mod db;
mod myenv;

use chunk::ends::{chunks_del, chunks_get, chunks_put};

pub type CreatedToChunk = Arc<RwLock<db::CreatedToChunk>>;

#[tokio::main]
async fn main() {
    // Enable env_logger implemenation of log.
    env_logger::init();

    let vars = myenv::get_vars();
    println!("{vars:?}");

    let db = Arc::new(RwLock::new(db::init().await));

    // Build router
    let app = Router::new()
        .route(
            "/chunks",
            get(chunks_get).put(chunks_put).delete(chunks_del),
        )
        .route("/", get(root))
        .merge(SpaRouter::new("/web", vars.web_dist.clone()))
        .layer(Extension(db.clone()))
        .layer(
            tower::ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        );

    // Create Socket to listen on
    let addr = SocketAddr::from_str(&vars.host).unwrap();
    info!("listening on {}", addr);

    // Create server
    let server = axum::Server::bind(&addr.into())
        .serve(app.into_make_service())
        .with_graceful_shutdown(async move {
            // Listen to iterrupt or terminate signal to order a shutdown if either is triggered
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

    let db = db.read().unwrap();
    db::save(&db).await;
}

async fn root() -> impl IntoResponse {
    ([(header::LOCATION, "/web")], StatusCode::MOVED_PERMANENTLY)
}


