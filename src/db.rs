use log::{error, info, trace};
use std::{collections::HashMap, fs};

use crate::{
    chunk::structs::{Chunk, UTC},
    myenv,
};

pub type CreatedToChunk = HashMap<UTC, Chunk>;

pub async fn init() -> CreatedToChunk {
    let vars = &myenv::get_vars();

    fn failover(path: &String) -> CreatedToChunk {
        info!("Reading {} failed, initializing with 0 chunks", path);
        CreatedToChunk::new()
    }
    // If db_init present, then attempt to connect to it's URL and initialize from it
    match &vars.db_init {
        Some(db_init) => {
            trace!("Fetching {db_init}");
            match reqwest::get(db_init).await {
                Ok(v) => {
                    let db_in = serde_json::from_slice::<Vec<Chunk>>(&v.bytes().await.unwrap())
                        .unwrap()
                        .into_iter()
                        .map(|v| (v.created.clone(), v))
                        .collect::<CreatedToChunk>();
                    info!("Read {} for {} chunks", db_init, db_in.len());
                    db_in
                }
                _ => failover(db_init),
            }
        }
        None => match fs::read_to_string(&vars.db_path) {
            Ok(db_json) => {
                let db_in = serde_json::from_str::<CreatedToChunk>(&db_json)
                    .expect(&format!("Couldn't read {}", vars.db_path));

                info!("Read {} for {} chunks", vars.db_path, db_in.len());

                db_in
            }
            _ => failover(&vars.db_path),
        },
    }
}
pub async fn save(db: &CreatedToChunk) {
    let vars = &myenv::get_vars();
    // let db_read = db.read().unwrap();
    let data = serde_json::to_string_pretty(db).unwrap();
    match fs::write(&vars.db_path, &data) {
        Ok(()) => info!("Saved {} chunks on {}", db.len(), &vars.db_path),
        Err(e) => {
            error!("Error saving to path {}: {e}", &vars.db_path);
            let backup_path = "chunks.backup.json".to_string();
            match fs::write(&backup_path, &data) {
                Ok(()) => info!("Saved {} chunks on backup {backup_path}", db.len()),
                Err(e) => error!("Error saving to backup path {backup_path}: {e}"),
            }
        }
    };
}
