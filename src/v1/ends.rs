use super::auth::UserClaims;

use super::db::db_chunk::DBChunk;
use super::db::ChunkView;
use super::format::value_to_html;
use super::socket::{ResourceMessage, ResourceSender};
use crate::utils::{MEDIA_FOLDER, PAGE_DIST};
use crate::v1::db::{Access, Chunk};
use crate::MediaEntry;
use crate::{utils::DbError, v1::*};
use axum::body::StreamBody;
use axum::extract::RawBody;
use axum::TypedHeader;
use axum::{
	extract::{Extension, Path},
	http::header,
	response::IntoResponse,
	Json,
};
use headers::ContentType;
use hyper::body::to_bytes;
use hyper::StatusCode;

use db;
use lazy_static::lazy_static;
use log::trace;
use proquint::Quintable;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
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

	let mut chunks: Vec<Chunk> = db
		.write()
		.unwrap()
		.get_chunks(&user_claims.user)
		.into_iter()
		.map(|v| v.read().unwrap().chunk().clone())
		.collect();
	chunks.sort_by_key(|v| -(v.modified as i64));

	trace!("GET /chunks len {}", chunks.len());

	Ok(Json(chunks))
}
pub async fn chunks_get_id(
	Path(id): Path<String>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, DbError> {
	if let Some(chunk) = db.read().unwrap().get_chunk(&id, &user_claims.user) {
		Ok(Json(chunk.read().unwrap().chunk().clone()))
	} else {
		Err(DbError::NotFound)
	}
}
pub async fn page_get_id(
	Path(id): Path<String>,
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
) -> Result<impl IntoResponse, DbError> {
	lazy_static! {
		static ref PAGE: String =
			std::fs::read_to_string(std::env::var("PAGE_DIST").unwrap_or("web".into()) + "/page.html").unwrap();
	};
	if let Some(chunk) = db.read().unwrap().get_chunk(&id, &user_claims.user) {
		let mut title = "Page".into();
		let mut html = "HTML".into();
		{
			let lock = chunk.read().unwrap();
			if let Some(v) = lock.get_prop::<String>("title") {
				title = v
			};
			html = value_to_html(&lock.chunk().value);
		}
		let page = PAGE.as_str();
		let page = page.replace("PAGE_TITLE", &title);
		let page = page.replace("PAGE_BODY", &html);
		Ok((TypedHeader(ContentType::html()), page))
	} else {
		Err(DbError::NotFound)
	}
}

// #[derive(Deserialize)]
// struct WellOptions {
//     compact: usize,
//     size: usize,
// }
// pub async fn well_get(
// 	id: Option<Path<String>>,
// 	Extension(db): Extension<DB>,
// 	Extension(user_claims): Extension<UserClaims>,
// ) -> Result<impl IntoResponse, DbError> {
// 	let root = id.and_then(|id| {
// 		db.read()
// 			.unwrap()
// 			.get_chunk(&id, &user_claims.user)
// 			.and_then(|v| Some(v.clone()))
// 	});
// 	let tree = db.write().unwrap().subtree(
// 		root.as_ref(),
// 		&user_claims.user.as_str().into(),
// 		&|v| v,
// 		&|node| {
// 			let node = node.read().unwrap();
// 			json!({"id": node.chunk().id})
// 		},
// 		0,
// 	);
// 	Ok(Json(tree))
// }

#[derive(Debug, Deserialize, Default)]
pub struct ChunkIn {
	id: Option<String>,
	value: String,
}

pub async fn chunks_put(
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
	Extension(tx_r): Extension<ResourceSender>,
	Json(body): Json<ChunkIn>,
) -> Result<impl IntoResponse, DbError> {
	let db_chunk = DBChunk::from((body.id.as_deref(), body.value.as_str(), user_claims.user.as_str()));
	let users = db_chunk.access_users();
	let users_to_notify = db.write().unwrap().set_chunk(db_chunk, &user_claims.user)?;

	// Notifies users for which access has changed
	// They should request an update of their active view that uses chunks
	// upon this request
	tx_r
		.send(ResourceMessage::from(("chunks", users_to_notify.clone())))
		.unwrap();

	// Notifies users which already have access, of the note's new content
	//
	// Only do so if modifying a chunk, because a new one won't have an id.
	// Because the user that created it will ask for them anyway almost immediately
	// since we will have told them that they have to update their view up there ^
	if let Some(id) = body.id {
		let chunk = ChunkView::from((
			db.read().unwrap().get_chunk(&id, &user_claims.user).unwrap(),
			user_claims.user.as_str(),
		));

		tx_r
			.send(ResourceMessage::from((
				format!("chunks/{}", id).as_str(),
				users,
				&chunk,
			)))
			.unwrap();
	}

	Ok(())
}

pub async fn chunks_del(
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
	Extension(tx_r): Extension<ResourceSender>,
	Json(input): Json<HashSet<String>>,
) -> Result<impl IntoResponse, DbError> {
	let users_to_notify = db.write().unwrap().del_chunk(input, &user_claims.user)?;

	tx_r.send(ResourceMessage::from(("chunks", users_to_notify))).unwrap();

	Ok(())
}

pub async fn media_get(Path(id): Path<String>) -> Result<impl IntoResponse, impl IntoResponse> {
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

		let headers = [
			(
				header::CONTENT_TYPE,
				match _type {
					Some(_type) => _type.mime_type(),
					None => "text/plain",
				},
			),
			(header::CACHE_CONTROL, "max-age=31536000"), // Makes browser cache for a year
		];
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
	// Extension(db): Extension<DB>,
	Extension(cache): Extension<Cache>,
	Extension(user_claims): Extension<UserClaims>,
	body: RawBody,
) -> Result<impl IntoResponse, impl IntoResponse> {
	let path = std::path::Path::new(MEDIA_FOLDER.as_str());
	if !path.exists() {
		fs::create_dir(&path).await.unwrap();
		info!("Created media folder");
	}

	let body = to_bytes(body.0).await.unwrap();
	let mut id;
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

/** Used as a magic static value for data cloning */
pub static MAGIC_BEAN: &'static str = "alkjgblnvcxlk_BANDFLKj";
/**
 * Endpoint allows other servers to clone this one's data
 */
pub async fn mirror_bean(
	Path(bean): Path<String>,
	Extension(db): Extension<self::ends::DB>,
) -> Result<impl IntoResponse, impl IntoResponse> {
	if bean == *MAGIC_BEAN {
		Ok(Json(DBData::from(&*db.read().unwrap())))
	} else {
		error!("Someone tried to access /mirror without bean.");
		Err("Who the F are you?")
	}
}
