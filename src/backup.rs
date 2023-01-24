use crate::{
	utils::{get_secs, DB_BACKUP_FOLDER, SECS_IN_DAY, SECS_IN_HOUR},
	v1::{db::DBData, ends::DB},
	Cache,
};
use log::{error, info};
use std::{
	fs,
	path::Path,
	sync::{Arc, RwLock},
	time::Duration,
};
use tokio::{sync::watch, time};

pub async fn backup_service(cache: Arc<RwLock<Cache>>, db: DB, mut shutdown_rx: watch::Receiver<()>) {
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
