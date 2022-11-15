use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};

use log::{error, trace};
use serde::Deserialize;

use crate::v0::structs::{Chunk, UTC};

pub type CreatedToChunk = Arc<RwLock<HashMap<UTC, Chunk>>>;

pub async fn chunks_get(Extension(db): Extension<CreatedToChunk>) -> impl IntoResponse {
	let chunks = db.read().unwrap();
	let mut chunks = chunks.values().cloned().collect::<Vec<_>>();
	chunks.sort_by_key(|c| c.created);
	trace!("GET /chunks len {}", chunks.len());
	Json(chunks)
}

#[derive(Debug, Deserialize)]
pub struct ChunkIn {
	value: String,
	created: Option<UTC>,
}

pub async fn chunks_put(Json(input): Json<ChunkIn>, Extension(db): Extension<CreatedToChunk>) -> impl IntoResponse {
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
			trace!(
				"PUT /chunks {}: {chunk:?}",
				if modified { "modified" } else { "created" }
			);
			Ok((if modified { StatusCode::OK } else { StatusCode::CREATED }, Json(chunk)))
		}
		Err(err) => {
			error!("PUT /chunks error, input: {input:?}");
			Err((
				StatusCode::NOT_ACCEPTABLE,
				format!("Chunk couldn't be created {:?}", err),
			))
		}
	}
}

pub async fn chunks_del(Json(input): Json<Vec<Chunk>>, Extension(db): Extension<CreatedToChunk>) -> impl IntoResponse {
	let mut db = db.write().unwrap();

	for chunk in &input {
		if !db.contains_key(&chunk.created) {
			return Err((StatusCode::NOT_FOUND, format!("{chunk:?}")));
		}
	}
	for chunk in &input {
		db.remove(&chunk.created);
	}

	trace!("DELETE /chunks {}", input.len());

	Ok(StatusCode::OK)
}
