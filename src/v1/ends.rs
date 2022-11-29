use super::auth::UserClaims;

use super::socket::{ResourceMessage, ResourceSender};
use crate::utils::MEDIA_FOLDER;
use crate::MediaEntry;
use crate::{utils::DbError, v1::*};
use axum::body::StreamBody;
use axum::extract::RawBody;
use axum::{
	extract::{Extension, Path},
	http::header,
	response::IntoResponse,
	Json,
};
use hyper::body::to_bytes;
use hyper::StatusCode;

use log::trace;
use proquint::Quintable;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufWriter, Cursor};
use tokio::fs;

use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

pub type DB = Arc<RwLock<db::DB>>;
pub type Cache = Arc<RwLock<crate::Cache>>;
impl IntoResponse for DbError {
	fn into_response(self) -> axum::response::Response {
		(StatusCode::FORBIDDEN, format!("{self:?}")).into_response()
	}
}

pub async fn chunks_get(
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, DbError> {
	info!("User is {}.", &user_claims.user);
	let mut notes = db.read().unwrap().get_notes(&user_claims.user);
	notes.sort_by_key(|v| -(v.0.modified as i64));
	trace!("GET /chunks len {}", notes.len());
	Ok(Json(notes))
}
pub async fn chunks_get_id(
	Path(id): Path<String>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, DbError> {
	let notes = db.read().unwrap().get_chunk(Some(user_claims.user), &id)?;

	Ok(Json(notes))
}

pub async fn well_get(
	id: Option<Path<String>>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, DbError> {
	let mut res = db
		.read()
		.unwrap()
		.get_chunks(user_claims.user, id.and_then(|v| Some(v.0)), None)?;
	res.0.sort_by_key(|v| -(v.0.modified as i64));

	Ok(Json(res))
}

#[derive(Debug, Deserialize)]
pub struct ChunkIn {
	id: Option<String>,
	value: String,
}

pub async fn chunks_put(
	Json(chunk_in): Json<ChunkIn>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
	Extension(tx_r): Extension<ResourceSender>,
) -> Result<impl IntoResponse, DbError> {
	// let mut db = ;
	let is_new = chunk_in.id.is_none();
	let (chunk, users, users_access_changed) = db
		.write()
		.unwrap()
		.set_chunk(&user_claims.user, (chunk_in.id, chunk_in.value))?;

	tx_r
		.send(if is_new {
			ResourceMessage::new::<()>(format!("chunks"), None, users)
		} else {
			ResourceMessage::new(format!("chunks/{}", chunk.id), Some(&chunk), users)
		})
		.unwrap();

	if users_access_changed.len() > 0 {
		tx_r
			.send(ResourceMessage::new::<()>(
				format!("chunks"),
				None,
				users_access_changed,
			))
			.unwrap();
	}

	Ok(Json(chunk))
}

pub async fn chunks_del(
	Json(input): Json<Vec<String>>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
	Extension(tx_r): Extension<ResourceSender>,
) -> Result<impl IntoResponse, DbError> {
	let chunks_changed = db.write().unwrap().del_chunk(&user_claims.user, input)?;

	// Notify user than wants to delete that view changed.
	tx_r
		.send(ResourceMessage::new::<()>(
			format!("chunks"),
			None,
			HashSet::from([user_claims.user]),
		))
		.unwrap();

	// Notify other users that these notes were modified
	chunks_changed.into_iter().for_each(|(c, m)| {
		let mut users = HashSet::<_>::default();
		users.insert(c.owner.to_owned());
		users.extend(m.access.into_iter().map(|(u, _)| u));
		tx_r
			.send(ResourceMessage::new(
				format!("chunks/{}", c.id.clone()),
				Some(&c),
				users,
			))
			.unwrap();
	});

	Ok(())
}

pub async fn media_get(
	Path(id): Path<String>,
	// Extension(db): Extension<DB>,
	// Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, impl IntoResponse> {
	// `File` implements `AsyncRead`
	let path = std::path::Path::new(MEDIA_FOLDER.as_str());
	let path = path.join(id);

	let mut file = match tokio::fs::File::open(&path).await {
		Ok(file) => file,
		Err(err) => return Err((StatusCode::NOT_FOUND, format!("File not found: {}", err))),
	};

	let mut buf = [0u8; 64];
	if let Ok(_size) = file.read(&mut buf).await {
		file.rewind().await.unwrap(); // Reset the counter to start of file
		let _type = infer::get(&buf);

		// // convert the `AsyncRead` into a `Stream`
		let stream = ReaderStream::new(file);

		// // convert the `Stream` into an `axum::body::HttpBody`
		let body = StreamBody::new(stream);

		let headers = match _type {
			Some(_type) => [(header::CONTENT_TYPE, _type.mime_type())],
			None => [(header::CONTENT_TYPE, "text/plain")],
		};
		Ok((headers, body))
	} else {
		Err((StatusCode::NO_CONTENT, "Error reading file?".to_string()))
	}
}

use std::collections::hash_map::DefaultHasher;

// So we can serialize infer::MatcherType, basically a copy of the enum
#[derive(Serialize, Deserialize)]
#[serde(remote = "infer::MatcherType")]
pub enum MatcherType {
	App,
	Archive,
	Audio,
	Book,
	Doc,
	Font,
	Image,
	Text,
	Video,
	Custom,
}

#[derive(Serialize)]
pub struct MediaPostResponse {
	id: String,

	#[serde(with = "MatcherType", rename = "type")]
	_type: infer::MatcherType,
}

/**
- [ ] Uploading to POST `api/media` will
	- create `data/media` if it doesn't exist
	- save under `data/media/<32bit_hash_proquint>`, return error `<hash> exists` if exists already, else, return `<hash>`.
*/
pub async fn media_post(
	Extension(db): Extension<DB>,
	Extension(cache): Extension<Cache>,
	Extension(user_claims): Extension<UserClaims>,
	body: RawBody,
) -> Result<impl IntoResponse, impl IntoResponse> {
	let path = std::path::Path::new(MEDIA_FOLDER.as_str());
	if !path.exists() {
		fs::create_dir(&path).await.unwrap();
		info!("Created media folder");
	}

	let mut body = to_bytes(body.0).await.unwrap();
	let mut id = "".to_string();
	{
		// Calculate hash
		let mut hasher = DefaultHasher::new();
		body.hash(&mut hasher);
		id = hasher.finish().to_quint();
	}

	// Do conversion if necessary
	let _type = infer::get(&body);
	let mut matcher_type = _type
		.and_then(|v| Some(v.matcher_type()))
		.unwrap_or(infer::MatcherType::Custom);

	// Don't perform conversion/file write if we have this id.
	let mut create = false;
	{
		let cache = cache.read().unwrap();
		if let Some(media_item) = cache.media.get(&id) {
			// let mut cache_item = cache_item.clone();
			// If we have a reference to a new conversion, make that the current id
			if let MediaEntry::Ref(id_cache) = media_item {
				if let Some(media_item) = cache.media.get(id_cache) {
					id = id_cache.clone();

					if let MediaEntry::Entry { user: _, _type } = media_item {
						matcher_type = *_type;
					} else {
						error!(
							"Media entry isn't Entry for {}? was referenced by {} that's weird",
							id, id_cache
						);
					}
				} else {
					create = true;
				}
			}
		} else {
			create = true
		}
	}

	if create {
		if let Some(_type) = _type {
			match _type.matcher_type() {
				// infer::MatcherType::Image => {
				// 	if let Ok(img) = image::load_from_memory(&body) {
				// 		let mut _body = BufWriter::new(Cursor::new(vec![]));
				// 		info!("Converting image w:{},h:{} to .avif", img.width(), img.height());
				// 		img.write_to(&mut _body, image::ImageOutputFormat::Avif).unwrap();
				// 		info!("Finished conversion of w:{},h:{}", img.width(), img.height());
				// 		body = _body.into_inner().unwrap().into_inner().into();
				// 	}
				// }
				_ => {}
			}
		}
		let id_in = id.clone();
		{
			// Calculate hash
			let mut hasher = DefaultHasher::new();
			body.hash(&mut hasher);
			id = hasher.finish().to_quint();
		}
		if id_in != id {
			// Means conversion changed the data
			cache
				.write()
				.unwrap()
				.media
				.insert(id_in, crate::MediaEntry::Ref(id.clone()));
		}
		let path = path.join(&id);

		if !path.exists() {
			if let Err(err) = fs::write(path, body).await {
				return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", err)));
			}
		}
		cache.write().unwrap().media.insert(
			id.clone(),
			crate::MediaEntry::Entry {
				user: user_claims.user,
				_type: matcher_type.clone(),
			},
		);
	}

	Ok(Json(MediaPostResponse {
		id,
		_type: matcher_type,
	}))
}

/** Used to validate that it's other servers that want this */
pub static MAGIC_BEAN: &'static str = "alkjgblnvcxlk_BANDFLKj";
pub async fn mirror_bean(
	Path(bean): Path<String>,
	Extension(db): Extension<self::ends::DB>,
) -> Result<impl IntoResponse, impl IntoResponse> {
	if bean == *MAGIC_BEAN {
		Ok(Json(DBData::new(&*db.read().unwrap())))
	} else {
		error!("Someone tried to access /mirror without bean.");
		Err("Who the F are you?")
	}
}
