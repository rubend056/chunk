use axum::{routing::get, Extension, Router};
use axum_extra::routing::SpaRouter;
use futures::{prelude::*, stream::FuturesUnordered};
use log::{error, info};
use std::{
	env,
	net::SocketAddr,
	str::FromStr,
	sync::{Arc, RwLock},
};
use tokio::signal::unix::{signal, SignalKind};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

mod utils;
mod v0;
mod v1;

// use V0::ends::{chunks_del, chunks_get, chunks_put};

use crate::utils::{HOST, WEB_DIST};
use crate::v1::ends::*;

#[tokio::main]
async fn main() {
	// Enable env_logger implemenation of log.
	env_logger::init();

	let db = Arc::new(RwLock::new(v1::init().await));

	let j = env::vars()
		.filter(|(k, v)| k.contains("REGEX_") || k.contains("DB_") || k == "HOST" || k == "WEB_DIST")
		.collect::<Vec<_>>();

	info!("{j:?}");

	// Build router
	let app = Router::new()
		.route(
			"/chunks",
			get(chunks_get).put(chunks_put).delete(chunks_del),
		)
		.merge(SpaRouter::new("/web", WEB_DIST.clone()))
		.layer(Extension(db.clone()))
		.layer(
			tower::ServiceBuilder::new()
				.layer(TraceLayer::new_for_http())
				.layer(CorsLayer::permissive()),
		);

	// Create Socket to listen on
	let addr = SocketAddr::from_str(&HOST).unwrap();
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

	if let Ok(db) = Arc::try_unwrap(db) {
		let db = db.into_inner().unwrap();
		v1::save(db).await;
	} else {
		error!("Couldn't unwrap DB");
	}
}
