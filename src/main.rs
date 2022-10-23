use axum::{routing::get, Extension, Router};
use axum_extra::routing::SpaRouter;
use futures::{prelude::*, pin_mut, future::{Either}};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::{
	env, fs,
	net::SocketAddr,
	path::Path,
	str::FromStr,
	sync::{Arc, RwLock},
	time::{Duration, SystemTime},
};
use tokio::{
	signal::unix::{signal, SignalKind},
	sync::watch,
	time,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utils::{get_secs, CACHE_PATH, DB_BACK_FOLDER};
use v1::db::DBData;

mod utils;
mod v0;
mod v1;

use crate::utils::{HOST, WEB_DIST};
use crate::v1::ends::*;

#[tokio::main]
async fn main() {
	// Enable env_logger implemenation of log.
	env_logger::init();

	// Read cache
	let cache = Arc::new(RwLock::new(init_cache()));

	let db = Arc::new(RwLock::new(v1::init().await));

	log_env();

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

	let (down_tx, mut down_rx) = watch::channel(());

	// Backup service
	let backup = tokio::spawn(backup_service(cache.clone(), db.clone(), down_rx.clone()));
	// Shutdown listener
	// tokio::spawn(shutdown_service(down_tx));

	// Create Socket to listen on
	let addr = SocketAddr::from_str(&HOST).unwrap();
	info!("listening on {}", addr);

	// Create server
	let server = axum::Server::bind(&addr.into())
		.serve(app.into_make_service())
		.with_graceful_shutdown(async move {
			if let Err(err) = down_rx.changed().await {
				error!("Error receiving shutdown {err:?}");
			}
		});
		
	let server = tokio::spawn(server);
		
	
	// Listen to iterrupt or terminate signal to order a shutdown if either is triggered
	let mut s0 = signal(SignalKind::interrupt()).unwrap();
	let mut s1 = signal(SignalKind::terminate()).unwrap();
	let f0 = s0.recv();pin_mut!(f0);
	let f1 = s1.recv();pin_mut!(f1);
	future::select(f0, f1).await;

	if let Err(err) = down_tx.send(()) {
		error!("Error sending shutdown {err:?}");
	} else {
		info!("Shutting down.");
	}
	let (r,_) = future::join(server,backup).await;
	if let Err(r) = r {
		error!("{r:?}");		
	}

	if let Ok(db) = Arc::try_unwrap(db) {
		let db = db.into_inner().unwrap();
		v1::save(db).await;
	} else {
		error!("Couldn't unwrap DB");
	}

	deinit_cache(Arc::try_unwrap(cache).unwrap().into_inner().unwrap());
}


fn log_env() {
	let j = env::vars()
		.filter(|(k, _)| k.contains("REGEX_") || k.contains("DB_") || k == "HOST" || k == "WEB_DIST")
		.collect::<Vec<_>>();

	info!("{j:?}");
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
struct Cache {
	pub last_backup: u64,
}
fn init_cache() -> Cache {
	fs::read(CACHE_PATH.clone())
		.and_then(|v| Ok(serde_json::from_slice::<Cache>(v.as_ref()).unwrap()))
		.unwrap_or_default()
}
fn deinit_cache(cache: Cache) {
	let cache = serde_json::to_string_pretty(&cache).unwrap();
	if let Err(err) = fs::write(CACHE_PATH.clone(), &cache) {
		error!("Couldn't write cache: {err:?}");
	}
}

// async fn shutdown_service(tx: watch::Sender<()>) {
// }
async fn backup_service(cache: Arc<RwLock<Cache>>, db: DB, mut rx: watch::Receiver<()>) {
	let backup_folder = Path::new(DB_BACK_FOLDER.as_str());
	if !backup_folder.is_dir() {
		fs::create_dir(backup_folder).unwrap();
		info!("Created {backup_folder:?}");
	}

	// if  fs::create_dir(path)
	// let mut last = 0;
	// {
	// 	last = cache.read().unwrap().last_backup;
	// }
	let sec_to_hrs:u64 = 60 * 60;
	let sec_to_days:u64= sec_to_hrs * 24;

	loop {
		let wait =
		// Last backup
			cache.read().unwrap().last_backup as i128 
			// Minus seconds now
			- get_secs() as i128 
			// Plus 2 days
			+ (sec_to_days as i128 * 2);
		
		
		if wait <= 0 {
			let secs = get_secs();
			cache.write().unwrap().last_backup = get_secs();

			let backup_file = backup_folder.join(format!("{}.json", (secs / sec_to_days) - (365*51)));
			let dbdata = serde_json::to_string_pretty(&DBData::new(&db.read().unwrap())).unwrap();
			if let Err(err) = fs::write(&backup_file, &dbdata) {
				error!("Couldn't write backup: {err:?}");
			} else {
				info!("Wrote {backup_file:?}");
			}
		} else {
			info!("Waiting {}h till next backup", wait / sec_to_hrs as i128);
			let f0 = time::sleep(Duration::from_secs(wait as u64));
			let f1 = rx.changed();
			pin_mut!(f0);
			pin_mut!(f1);
			match future::select(f0, f1).await {
				Either::Left(_) => continue,
				Either::Right(_) => break,
			}
		}
	}
}
