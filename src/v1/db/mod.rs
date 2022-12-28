use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
/** Designing a new Data Structure that would allow for all queries/insertions/serializations to efficiently happen */

/**
 - Different visualization options
 - Editing
	 - ![](web/src/assets/icons/card-text.svg) **Shank/Edit** -> selected chunk + children up to 4N (1N default) an editor
 - Viewing
	 - ![](web/src/assets/icons/clipboard.svg) **Notes** -> chunks ordered by recent side by side
	 - ![](web/src/assets/icons/grid.svg) **Well** -> selected chunk children on a grid
	 - ![](web/src/assets/icons/diagram-2-fill.svg) **Graph** -> nodes in a tree
				 (S)	-> (R r)
				(S) -> ()

				(R) -> (S w) ->
								\> (S r)
		Querying this with different views
*/
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	sync::{Arc, RwLock, RwLockWriteGuard},
};

pub type DBMap<K, V> = BTreeMap<K, V>;

use crate::{
	utils::{gen_proquint, get_secs},
	v1::user::User,
};

use self::db_chunk::DBChunk;

/**
 * ChunkView is meant for specific Chunk Data
 * It turns an Rc<DBChunk> to an a specific View of it.
 * This will be customizable based on what the UI needs.
 */
#[derive(Serialize, Debug, Default)]
pub struct ChunkView {
	pub id: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub owner: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub created: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub modified: Option<u64>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub props: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub props_dynamic: Option<Value>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub parents: Option<usize>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub children: Option<usize>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub access: Option<Access>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ViewType {
	Edit,
	Notes,
	Well,
	Graph,
}
impl From<(Arc<RwLock<DBChunk>>, &str, ViewType)> for ChunkView {
	fn from((rc, user, view_type): (Arc<RwLock<DBChunk>>, &str, ViewType)) -> Self {
		Self::from((&rc, user, view_type))
	}
}
impl From<(&Arc<RwLock<DBChunk>>, &str, ViewType)> for ChunkView {
	fn from((rc, user, view_type): (&Arc<RwLock<DBChunk>>, &str, ViewType)) -> Self {
		let mut db_chunk = rc.write().unwrap();
		let value_short = |db_chunk: &RwLockWriteGuard<DBChunk>| {
			let mut v = 0;
			db_chunk
				.chunk()
				.value
				.chars()
				.take_while(|c| {
					if v == 10 {
						return false;
					};
					if *c == '\n' {
						v += 1;
					};
					true
				})
				.collect::<String>()
		};
		if user == "public" {
			Self {
				id: db_chunk.chunk().id.clone(),
				props: Some(Value::Object(Map::from_iter(db_chunk.props()))),
				value: Some(db_chunk.chunk().value.clone()),
				..Default::default()
			}
		} else {
			match view_type {
				ViewType::Well => Self {
					id: db_chunk.chunk().id.clone(),

					props: Some(Value::Object(Map::from_iter(db_chunk.props()))),
					props_dynamic: Some(Value::Object(Map::from_iter(db_chunk.props_dynamic(&user.into())))),

					value: Some(value_short(&db_chunk)),

					owner: Some(db_chunk.chunk().owner.clone()),
					modified: Some(db_chunk.chunk().modified),
					created: Some(db_chunk.chunk().created),

					parents: Some(db_chunk.parents(Some(&user.into())).len()),
					children: Some(db_chunk.children(Some(&user.into())).len()),

					access: db_chunk
						.highest_access(user)
						.and_then(|a| if a == Access::Owner { None } else { Some(a) }),
					..Default::default()
				},
				ViewType::Graph => Self {
					id: db_chunk.chunk().id.clone(),
					created: Some(db_chunk.chunk().created),

					props: Some(Value::Object(Map::from_iter(db_chunk.props()))),
					props_dynamic: Some(Value::Object(Map::from_iter(db_chunk.props_dynamic(&user.into())))),

					parents: Some(db_chunk.parents(Some(&user.into())).len()),
					children: Some(db_chunk.children(Some(&user.into())).len()),
					..Default::default()
				},
				ViewType::Notes => Self {
					id: db_chunk.chunk().id.clone(),
					modified: Some(db_chunk.chunk().modified),

					// props: Some(Value::Object(Map::from_iter(db_chunk.props()))),
					// props_dynamic: Some(Value::Object(Map::from_iter(db_chunk.props_dynamic(&user.into())))),
					value: Some(value_short(&db_chunk)),

					// children: db_chunk.children(Some(&user.into())).len(),
					access: db_chunk
						.highest_access(user)
						.and_then(|a| if a == Access::Owner { None } else { Some(a) }),
					..Default::default()
				},
				ViewType::Edit => Self {
					id: db_chunk.chunk().id.clone(),
					props: Some(Value::Object(Map::from_iter(db_chunk.props()))),
					props_dynamic: Some(Value::Object(Map::from_iter(db_chunk.props_dynamic(&user.into())))),
					// value: Some(db_chunk.chunk().value.clone()),
					owner: Some(db_chunk.chunk().owner.clone()),
					parents: Some(db_chunk.parents(Some(&user.into())).len()),
					children: Some(db_chunk.children(Some(&user.into())).len()),
					modified: Some(db_chunk.chunk().modified),
					created: Some(db_chunk.chunk().created),
					// access: db_chunk
					// 	.highest_access(user)
					// 	.and_then(|a| if a == Access::Owner { None } else { Some(a) }),
					..Default::default()
				},
			}
		}
	}
}
impl From<(Arc<RwLock<DBChunk>>, &str)> for ChunkView {
	fn from((rc, user): (Arc<RwLock<DBChunk>>, &str)) -> Self {
		Self::from((rc, user, ViewType::Edit))
	}
}

/**
 * ChunkId is meant for Views
 * It turns an Rc<DBChunk> to an Id String
 */
#[derive(Serialize)]
pub struct ChunkId(String);
impl From<Arc<RwLock<DBChunk>>> for ChunkId {
	fn from(rc: Arc<RwLock<DBChunk>>) -> Self {
		Self::from(&rc)
	}
}
impl From<&Arc<RwLock<DBChunk>>> for ChunkId {
	fn from(rc: &Arc<RwLock<DBChunk>>) -> Self {
		Self(rc.read().unwrap().chunk().id.clone())
	}
}
/**
 * ChunkValue
 * It turns an Rc<DBChunk> to a Value String
 */
#[derive(Serialize)]
pub struct ChunkValue(String);
impl From<Arc<RwLock<DBChunk>>> for ChunkValue {
	fn from(rc: Arc<RwLock<DBChunk>>) -> Self {
		Self::from(&rc)
	}
}
impl From<&Arc<RwLock<DBChunk>>> for ChunkValue {
	fn from(rc: &Arc<RwLock<DBChunk>>) -> Self {
		Self(rc.read().unwrap().chunk().value.clone())
	}
}
pub enum SortType {
	Modified,
	ModifiedDynamic(UserAccess),
	Created,
}
pub struct ChunkVec(pub Vec<Arc<RwLock<DBChunk>>>);
impl ChunkVec {
	pub fn sort(&mut self, t: SortType) {
		self.0.sort_by_key(|v| {
			-(match &t {
				SortType::Created => v.read().unwrap().chunk().created,
				SortType::Modified => v.read().unwrap().chunk().modified,
				SortType::ModifiedDynamic(ua) => v.write().unwrap().get_prop_dynamic("modified", &ua).unwrap_or_default(),
			} as i64)
		})
	}
}
impl From<Vec<Arc<RwLock<DBChunk>>>> for ChunkVec {
	fn from(v: Vec<Arc<RwLock<DBChunk>>>) -> Self {
		Self(v)
	}
}
impl<T: From<Arc<RwLock<DBChunk>>>> Into<Vec<T>> for ChunkVec {
	fn into(self) -> Vec<T> {
		self.0.into_iter().map(|v| v.into()).collect()
	}
}

/**
 * The basic building block
 */
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
	pub id: String,
	pub value: String,
	pub owner: String,
	pub created: u64,
	pub modified: u64,
}
impl Default for Chunk {
	fn default() -> Self {
		let secs = get_secs();
		Self {
			id: gen_proquint(),
			value: Default::default(),
			owner: Default::default(),
			created: secs,
			modified: secs,
		}
	}
}
impl PartialEq for Chunk {
	fn eq(&self, other: &Self) -> bool {
		self.id == other.id && self.owner == other.owner && self.created == other.created
	}
}
/**
 * Value
 */
impl From<&str> for Chunk {
	fn from(value: &str) -> Self {
		Self {
			value: value.to_owned(),
			..Default::default()
		}
	}
}
/**
 * (Id, Value)
 */
impl From<(&str, &str)> for Chunk {
	fn from((id, value): (&str, &str)) -> Self {
		Self {
			id: id.to_owned(),
			value: value.to_owned(),
			..Default::default()
		}
	}
}
/**
 * (Id, Value, Owner)
 */
impl From<(&str, &str, &str)> for Chunk {
	fn from((id, value, owner): (&str, &str, &str)) -> Self {
		Self::from((Some(id), value, owner))
	}
}
/**
 * (Id?, Value, Owner)
 */
impl From<(Option<&str>, &str, &str)> for Chunk {
	fn from((id, value, owner): (Option<&str>, &str, &str)) -> Self {
		Self {
			id: id.and_then(|v| Some(v.into())).unwrap_or(gen_proquint()),
			value: value.to_owned(),
			owner: owner.to_owned(),
			..Default::default()
		}
	}
}

#[derive(Serialize, Deserialize, Hash, Eq, PartialOrd, Ord, PartialEq, Clone, Debug, Default)]
pub enum Access {
	#[default]
	Read,
	Write,
	Admin,
	Owner,
}
#[derive(Serialize, Deserialize, Hash, Eq, PartialOrd, Ord, PartialEq, Clone, Debug, Default)]
pub struct UserAccess {
	pub user: String,
	pub access: Access,
}
impl From<(String, Access)> for UserAccess {
	fn from((user, access): (String, Access)) -> Self {
		Self { user, access }
	}
}
impl From<(&str, Access)> for UserAccess {
	fn from((user, access): (&str, Access)) -> Self {
		Self::from((user.to_string(), access))
	}
}
impl From<&str> for UserAccess {
	fn from(user: &str) -> Self {
		Self::from((user.to_string(), Access::default()))
	}
}

#[derive(Serialize, Debug)]
pub struct GraphView(Value, Vec<GraphView>);

/**
 * An improved 2.0, reference counted version,
 * Very much an improvement over the
 * last RAM DB representation that used lookups for everything.
 */
#[derive(Default)]
pub struct DB {
	pub auth: db_auth::DBAuth,
	chunks: DBMap<String, Arc<RwLock<DBChunk>>>,
}
/**
 * DB data that will acutally get stored on disk
 */
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DBData {
	pub chunks: Vec<Chunk>,
	pub users: Vec<User>,
	pub groups: HashMap<String, HashSet<String>>,
}

mod db_app;
mod db_auth;
pub mod db_chunk;

#[cfg(test)]
mod tests;
