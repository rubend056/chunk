use axum::{
	extract::DefaultBodyLimit,
	routing::{get, get_service, post},
	Extension, Router,
};
use futures::future::join;
use hyper::StatusCode;
use lib::{
	backup::backup_service,
	utils::{get_secs, log_env, CACHE_PATH, DB_BACKUP_FOLDER, HOST, SECS_IN_DAY, SECS_IN_HOUR, WEB_DIST},
	v1::{
		self, auth,
		db::DBData,
		ends::{self, DB},
		socket::{websocket_handler, ResourceMessage},
	},
	Cache,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{
	collections::HashMap,
	env, fs,
	future::ready,
	net::SocketAddr,
	path::{Path, PathBuf},
	str::FromStr,
	sync::{Arc, RwLock},
	time::Duration,
};
use tokio::{
	signal::unix::{signal, SignalKind},
	sync::{broadcast, watch},
	time,
};
use tower_http::{
	services::{ServeDir, ServeFile},
	timeout::TimeoutLayer,
	trace::TraceLayer,
};

#[tokio::main]
async fn main() {
	// Enable env_logger implemenation of log.
	println!("Running chunk-app");
	env_logger::init();
	log_env();

	// Read cache
	let cache = Arc::new(RwLock::new(Cache::init()));

	let db = Arc::new(RwLock::new(v1::init().await));

	let (shutdown_tx, mut shutdown_rx) = watch::channel(());
	let (resource_tx, _resource_rx) = broadcast::channel::<ResourceMessage>(16);

	let spa = || {
		let assets_dir = PathBuf::from(WEB_DIST.clone());
		let assets_path = "/web";
		let index_file = assets_dir.join("index.html");
		let assets_service = get_service(ServeDir::new(&assets_dir).precompressed_br())
			.handle_error(|_| ready(StatusCode::INTERNAL_SERVER_ERROR));

		Router::new()
			.nest_service(&assets_path, assets_service)
			.fallback_service(
				get_service(ServeFile::new(index_file)).handle_error(|_| ready(StatusCode::INTERNAL_SERVER_ERROR)),
			)
	};

	// Build router
	let app = Router::new()
		.route("/page/:id", get(ends::page_get_id))
		.nest(
			"/api",
			Router::new()
				.route(
					"/chunks",
					get(ends::chunks_get).put(ends::chunks_put).delete(ends::chunks_del),
				)
				.route("/media", post(ends::media_post))
				// ONLY if NOT public ^
				.route_layer(axum::middleware::from_fn(auth::auth_required))
				.route("/chunks/:id", get(ends::chunks_get_id))
				.route("/stream", get(websocket_handler))
				.route("/user", get(auth::user))
				.route("/media/:id", get(ends::media_get))
				.route("/logout_all", get(auth::logout_all))
				// ONLY GET if public ^
				.route_layer(axum::middleware::from_fn(auth::public_only_get))
				.route("/login", post(auth::login))
				.route("/reset", post(auth::reset))
				.route("/register", post(auth::register))
				.route("/mirror/:bean", get(ends::mirror_bean)),
		)
		// User authentication, provider of UserClaims
		.route_layer(axum::middleware::from_fn(auth::authenticate))
		.merge(spa())
		.layer(
			tower::ServiceBuilder::new()
				.layer(TraceLayer::new_for_http())
				.layer(DefaultBodyLimit::disable())
				.layer(TimeoutLayer::new(Duration::from_secs(30)))
				.layer(Extension(db.clone()))
				.layer(Extension(cache.clone()))
				.layer(Extension(shutdown_rx.clone()))
				.layer(Extension(resource_tx.clone())),
		);

	// Backup service
	let backup = tokio::spawn(backup_service(cache.clone(), db.clone(), shutdown_rx.clone()));

	// Create Socket to listen on
	let addr = SocketAddr::from_str(&HOST).unwrap();
	info!("Listening on '{}'.", addr);

	// Create server
	let server = axum::Server::bind(&addr.into())
		.serve(app.into_make_service_with_connect_info::<SocketAddr>())
		.with_graceful_shutdown(async move {
			if let Err(err) = shutdown_rx.changed().await {
				error!("Error receiving shutdown {err:?}");
			} else {
				info!("Http server shutting down gracefully");
			}
		});

	let server = tokio::spawn(server);

	// Listen to iterrupt or terminate signal to order a shutdown if either is triggered
	let mut s0 = signal(SignalKind::interrupt()).unwrap();
	let mut s1 = signal(SignalKind::terminate()).unwrap();
	tokio::select! {
		_ = s0.recv() => {
			info!("Received Interrupt, exiting.");
		}
		_ = s1.recv() => {
			info!("Received Terminate, exiting.");
		}
	}

	info!("Telling everyone to shutdown.");
	shutdown_tx.send(()).unwrap();

	info!("Waiting for everyone to shutdown.");
	let (_server_r, _backup_r) = join(server, backup).await;

	info!("Joined workers, apparently they've shutdown");

	if db.is_poisoned() {
		error!(
			"DB was poisoned, can't clear it because we're in (stable) channel; so saving won't work.\n\
			This probaly happened because of an error.\n\
			Logging service will soon be implemented to notify of these."
		);
		// db.clear_poison();
	}

	match Arc::try_unwrap(db) {
		Ok(db) => {
			let db = db.into_inner().unwrap();
			v1::save(&db).await;
		}
		Err(db) => {
			error!("Couldn't unwrap DB, will save anyways, but beware of this");
			v1::save(&db.read().unwrap()).await;
		}
	}

	cache.read().unwrap().save();
}
