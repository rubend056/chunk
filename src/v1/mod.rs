use log::{error, info, trace};
use std::fs;

use crate::utils::{DB_INIT, DB_PATH};

use self::{
	db::{DBData, DB},
	ends::MAGIC_BEAN,
};

pub mod auth;
// pub mod chunk;
pub mod format;
pub mod db;
pub mod ends;
pub mod js_deno;
pub mod js_quick;
pub mod socket;
pub mod user;

pub async fn init() -> DB {
	fn failover(path: &str) -> DB {
		info!("Reading {} failed, initializing empty DB", path);
		DB::default()
	}
	// failover("");
	// If db_init present, then attempt to connect to it's URL and initialize from it
	match DB_INIT.as_ref() {
		Some(db_init) => {
			trace!("Fetching {}", db_init);
			match reqwest::get(format!("{db_init}/api/mirror/{MAGIC_BEAN}")).await {
				Ok(v) => {
					let dbdata = serde_json::from_slice::<DBData>(&v.bytes().await.unwrap()).unwrap();
					info!(
						"Read {} for {} chunks, and {} users",
						&db_init,
						dbdata.chunks.len(),
						dbdata.users.len()
					);
					DB::from(dbdata)
				}
				_ => failover(&db_init),
			}
		}
		None => match DB_PATH.clone() {
			Some(db_path) => match fs::read_to_string(&db_path) {
				Ok(db_json) => {
					let db_in = serde_json::from_str::<DBData>(&db_json).expect(&format!("Couldn't read {}", &db_path));

					info!("Read {} for {} chunks", &db_path, db_in.chunks.len());

					db_in.into()
				}
				_ => failover(&db_path),
			},
			None => failover("None"),
		},
	}
}
pub async fn save(db: &DB) {
	if let Some(db_path) = DB_PATH.clone() {
		let dbdata = DBData::from(db);
		let data = serde_json::to_string(&dbdata).unwrap();
		match fs::write(&db_path, &data) {
			Ok(()) => info!("Saved {} chunks on {}", dbdata.chunks.len(), db_path),
			Err(e) => {
				error!("Error saving to path {}: {e}", &db_path);
				let backup_path = "chunks.backup.json".to_string();
				match fs::write(&backup_path, &data) {
					Ok(()) => info!("Saved {} chunks on backup {backup_path}", dbdata.chunks.len()),
					Err(e) => error!("Error saving to backup path {backup_path}: {e}"),
				}
			}
		};
	}
}
