// #![feature(is_some_and)]
// #![feature(map_many_mut)]
// #![allow(dead_code)]

use axum::{
	extract::DefaultBodyLimit,
	routing::{get, post},
	Extension, Router,
};
use axum_extra::routing::SpaRouter;
use futures::future::join;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{
	collections::HashMap,
	env, fs,
	net::SocketAddr,
	path::Path,
	str::FromStr,
	sync::{Arc, RwLock},
	time::Duration,
};
use tokio::{
	signal::unix::{signal, SignalKind},
	sync::{broadcast, watch},
	time,
};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use utils::{get_secs, CACHE_PATH, DB_BACKUP_FOLDER, SECS_IN_DAY};
use v1::{db::DBData, socket::websocket_handler};

mod utils;
mod v1;

use crate::{
	utils::SECS_IN_HOUR,
	v1::{auth, ends::*},
};
use crate::{
	utils::{HOST, WEB_DIST},
	v1::socket::ResourceMessage,
};

#[tokio::main]
async fn main() {
	// Enable env_logger implemenation of log.
	env_logger::init();
	log_env();

	// Read cache
	let cache = Arc::new(RwLock::new(init_cache()));

	let db = Arc::new(RwLock::new(v1::init().await));

	let (shutdown_tx, mut shutdown_rx) = watch::channel(());
	let (resource_tx, _resource_rx) = broadcast::channel::<ResourceMessage>(16);

	// Build router
	let app = Router::new()
		.route("/page/:id", get(page_get_id))
		.nest(
			"/api",
			Router::new()
				.route("/chunks", get(chunks_get).put(chunks_put).delete(chunks_del))
				.route("/media", post(media_post))
				// ONLY if NOT public ^
				.route_layer(axum::middleware::from_fn(auth::auth_required))
				.route("/chunks/:id", get(chunks_get_id))
				.route("/stream", get(websocket_handler))
				.route("/user", get(auth::user))
				.route("/media/:id", get(media_get))
				.route("/logout_all", get(auth::logout_all))
				// ONLY GET if public ^
				.route_layer(axum::middleware::from_fn(auth::public_only_get))
				.route("/login", post(auth::login))
				.route("/reset", post(auth::reset))
				.route("/register", post(auth::register))
				.route("/mirror/:bean", get(mirror_bean)),
		)
		// User authentication, provider of UserClaims
		.route_layer(axum::middleware::from_fn(auth::authenticate))
		.merge(SpaRouter::new("/web", WEB_DIST.clone()))
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

	deinit_cache(&cache.read().unwrap());
}

fn log_env() {
	let j = env::vars().filter(|(k, _)| k.contains("REGEX_") || k.contains("DB_") || k == "HOST" || k == "WEB_DIST");
	j.for_each(|(k, v)| println!("{k}: {v}"));
	// info!(
	// 	"{:?}",
	// 	j.fold("\n".to_string(), |mut acc, (k, v)| {
	// 		acc.push_str(&format!("{k}:{v}\n"));
	// 		acc
	// 	})
	// );
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MediaEntry {
	Ref(String), // Means entry hash maps to another hash, meaning conversion yielded a different hash
	Entry {
		user: String,
		#[serde(with = "v1::ends::MatcherType", rename = "type")]
		_type: infer::MatcherType,
	},
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Cache {
	pub last_backup: u64,
	pub media: HashMap<String, MediaEntry>,
}
fn init_cache() -> Cache {
	fs::read(CACHE_PATH.clone())
		.and_then(|v| Ok(serde_json::from_slice::<Cache>(v.as_ref()).unwrap()))
		.unwrap_or_default()
}
fn deinit_cache(cache: &Cache) {
	let cache = serde_json::to_string_pretty(cache).unwrap();
	if let Err(err) = fs::write(CACHE_PATH.clone(), &cache) {
		error!("Couldn't write cache: {err:?}");
	}
}

async fn backup_service(cache: Arc<RwLock<Cache>>, db: DB, mut shutdown_rx: watch::Receiver<()>) {
	let backup_folder = Path::new(DB_BACKUP_FOLDER.as_str());
	if !backup_folder.is_dir() {
		fs::create_dir(backup_folder).unwrap();
		info!("Created {backup_folder:?}.");
	}

	loop {
		let wait =
		// Last backup
			cache.read().unwrap().last_backup as i128
			// Minus seconds now
			- get_secs() as i128
			// Plus 2 hours
			+ (SECS_IN_HOUR as i128 * 2);

		if wait <= 0 {
			let secs = get_secs();
			cache.write().unwrap().last_backup = get_secs();

			let backup_file = backup_folder.join(format!(
				"{}.json",
				(secs / SECS_IN_DAY) - (365 * 51) /*Closest number to days since EPOCH to lower that to something more readable */
			));

			let dbdata = serde_json::to_string(&DBData::from(&*db.read().unwrap())).unwrap();

			if let Err(err) = fs::write(&backup_file, &dbdata) {
				error!("Couldn't backup to: {err:?}");
			} else {
				info!("Backed up to {backup_file:?}.");
			}
		} else {
			info!("Waiting {}h till next backup", wait / SECS_IN_HOUR as i128);
			tokio::select! {
				_ = time::sleep(Duration::from_secs(wait as u64)) => {
					continue;
				}
				_ = shutdown_rx.changed() => {
					break;
				}
			}
		}
	}
}
