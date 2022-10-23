use super::db::ChunkDel;
use crate::v1::*;
use axum::{extract::Extension, response::IntoResponse, Json};
use log::trace;
use serde::Deserialize;
use std::sync::{Arc, RwLock};

pub type DB = Arc<RwLock<db::DB>>;

pub async fn chunks_get(Extension(db): Extension<DB>) -> impl IntoResponse {
	let db = db.read().unwrap();
	let mut notes = db.get_notes("rubend".to_string());
	notes.sort_by_key(|v| v.0.modified);
	trace!("GET /chunks len {}", notes.len());
	Json(notes)
}

#[derive(Debug, Deserialize)]
pub struct ChunkIn {
	id: Option<String>,
	value: String,
}

pub async fn chunks_put(
	Json(chunk_in): Json<ChunkIn>,
	Extension(db): Extension<DB>,
) -> impl IntoResponse {
	let mut db = db.write().unwrap();

	Json(db.set_chunk("rubend".to_string(), (chunk_in.id, chunk_in.value)))
}

pub async fn chunks_del(
	Json(input): Json<ChunkDel>,
	Extension(db): Extension<DB>,
) -> impl IntoResponse {
	let mut db = db.write().unwrap();

	Json(db.del_chunk(&"rubend".to_string(), input))
}
