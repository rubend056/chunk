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
use std::collections::{HashMap, HashSet};

use log::{error, info};
use serde::{Deserialize, Serialize};

use crate::{
	utils::{gen_proquint, get_secs, DbError},
	v1::{
		chunk::{Chunk, ChunkMeta},
		user::User,
	},
};

use super::chunk::{standardize, Access, UserAccess};

#[derive(Serialize, Deserialize)]
pub struct DBData {
	pub chunks: Vec<Chunk>, // proquint (id) -> Chunk
	pub users: Vec<User>,   // user -> User
}
impl From<DB> for DBData {
	fn from(value: DB) -> Self {
		DBData {
			chunks: value.chunks.into_iter().map(|c| c.1 .0).collect(),
			users: value.users.into_iter().map(|c| c.1).collect(),
		}
	}
}
impl DBData {
	pub fn new(value: &DB) -> Self {
		DBData {
			chunks: value.chunks.iter().map(|c| c.1 .0.clone()).collect(),
			users: value.users.iter().map(|c| c.1.clone()).collect(),
		}
	}
}

#[derive(Serialize)]
pub enum ChunkType {
	Owner,
	Access(Access),
}
pub type ChunkView = (Chunk, ChunkType);
#[derive(Serialize)]
pub struct ChunkTree(pub Chunk, pub Option<Vec<ChunkTree>>);

type ChunkAndMeta = (Chunk, ChunkMeta);
type UsersToNotify = HashSet<String>;

#[derive(Default, Debug, Serialize)]
pub struct DB {
	// _chunks: Vec<>,
	chunks: HashMap<String, ChunkAndMeta>,
	users: HashMap<String, User>,
	// tree: HashMap<String, Vec<String>>,
	/* rubend_read -> sara_notes */ // Updating is a pain bc we need to see who
	// access: HashMap<UserAccess, HashSet<UserRef>> // An update would remove
	/* I need a structure that's fast to modify/lookup */
	/*
	 Should we separate access to different structure?
	 Since we always come with a user, we always need to know what a user has access to,
	 but... since a note's access is inhereted we'd need to change this no matter what.
	 Best thing is to move through the tree on every query and keep in memory who when were has access to what.
	*/
	// access:
}
impl From<DBData> for DB {
	fn from(value: DBData) -> Self {
		DB {
			chunks: value
				.chunks
				.into_iter()
				.map(|chunk| (chunk.id.clone(), (chunk.clone(), ChunkMeta::from(&chunk.value))))
				.collect(),
			users: value.users.into_iter().map(|user| (user.user.clone(), user)).collect(),
		}
	}
}

impl DB {
	pub fn new_user(&mut self, user: String, pass: String) -> Result<(), DbError> {
		if self.users.get(&user).is_some() {
			return Err(DbError::UserTaken);
		}

		let user = User::new(user.clone(), pass)?;

		if user.user == "public" {
			return Err(DbError::InvalidUser);
		}

		self.users.insert(user.user.clone(), user);
		Ok(())
	}
	pub fn login(&self, user: &str, pass: &str) -> Result<(), DbError> {
		let user = self.users.get(user).ok_or(DbError::AuthError)?;
		if !user.verify(&pass) {
			return Err(DbError::AuthError);
		}
		Ok(())
	}
	pub fn reset(&mut self, user: &str, pass: &str, old_pass: &str) -> Result<(), DbError> {
		let user = self.users.get_mut(user).ok_or(DbError::AuthError)?;

		user.reset_pass(&old_pass, &pass)
	}

	fn iter_tree(&self, ua: &UserAccess, root: Option<(&Chunk, &ChunkMeta)>, depth: u32) -> Vec<ChunkTree> {
		self
			.chunks
			.iter()
			.filter(|(_, (chunk, meta))| {
				(meta.access.contains(ua) || chunk.owner == ua.0)
					&& match root {
						Some((chunk_root, meta_root)) => {
							meta
								._refs
								// Check if any chunk is referencing our owner & reference
								.contains(&(Some(chunk_root.owner.clone()), meta_root._ref.clone()))
								// Or, if the chunk's owners are the same, also check if any refs without owner point to root's
								|| ( chunk_root.owner == chunk.owner && meta._refs.contains(&(None, meta_root._ref.clone()))) 
								// Or, if it contains our id
								|| meta._refs.contains(&(None, chunk_root.id.clone()))
						}
						None => meta._refs.is_empty(),
					}
			})
			.map(|(_, (chunk, meta))| {
				ChunkTree(
					chunk.clone(),
					if depth > 0 {
						Some(self.iter_tree(ua, Some((chunk, meta)), depth - 1))
					} else {
						None
					},
				)
			})
			.collect()
	}
	/**
	 * Depth 0 => roots
	 * Depth 1 => roots -> children, ...
	 */
	pub fn get_chunks(
		&self,
		user: String,
		root: Option<String>,
		depth: Option<u32>,
	) -> Result<(Vec<ChunkTree>, Option<(Chunk, ChunkMeta)>), DbError> {
		match root {
			Some(root) => {
				let (chunk, meta) = self._get_chunk(Some(user.clone()), &root)?;
				// Some((, chunk))
				Ok((
					self.iter_tree(&(user, Access::Read), Some((&chunk, &meta)), depth.unwrap_or(0)),
					Some((chunk, meta)),
				))
			}
			_ => Ok((self.iter_tree(&(user, Access::Read), None, depth.unwrap_or(0)), None)),
		}
	}
	pub fn get_notes(&self, user: &str) -> Vec<ChunkView> {
		let access = (user.to_owned(), Access::Read);
		self.chunks.iter().fold(vec![], |mut acc, (_, (chunk, meta))| {
			if chunk.owner == *user {
				acc.push((chunk.to_owned(), ChunkType::Owner));
			} else if meta.access.contains(&access) {
				acc.push((
					chunk.to_owned(),
					ChunkType::Access(if meta.access.contains(&(user.to_owned(), Access::Write)) {
						Access::Write
					} else {
						Access::Read
					}),
				));
			};
			acc
		})
	}
	fn _get_chunk(&self, user: Option<String>, id_or_ref: &String) -> Result<(Chunk, ChunkMeta), DbError> {
		let user = user.unwrap_or("public".into());
		let id_or_ref = standardize(id_or_ref);

		if let Some((chunk, meta)) = self.chunks.get(&id_or_ref).or_else(|| {
			self
				.chunks
				.iter()
				.find(|(_, (_, meta))| meta._ref == id_or_ref)
				.map(|v| v.1)
		}) {
			if chunk.owner == user || meta.access.contains(&(user, Access::Read)) {
				return Ok((chunk.clone(), meta.clone()));
			}
		}
		Err(DbError::NotFound)
	}

	pub fn get_chunk(&self, user: Option<String>, id_or_ref: &String) -> Result<Chunk, DbError> {
		Ok(self._get_chunk(user, id_or_ref)?.0)
	}


	pub fn set_chunk(
		&mut self,
		user: &str,
		(id, value): (Option<String>, String),
	) -> Result<(Chunk, UsersToNotify, UsersToNotify), DbError> {
		let meta_new = ChunkMeta::from(&value);
		// if meta_new._ref.is_empty() {
		// 	return Err(DbError::InvalidChunk);
		// }

		match id {
			Some(id) => {
				// Create a new note with info the user gave us
				// let mut chunk_new = Chunk::new(id, value, user)?;
				// Calculate it's metadata

				// Make sure user can do what he wants
				match self.chunks.get_mut(&id) {
					Some((chunk, meta)) => {
						if chunk.owner == user {
							// If user is the owner, then allow the change
						} else {
							// If user isn't the owner, then do strict checks
							if !meta.access.contains(&(user.into(), Access::Write)) {
								error!("User {} doesn't have write access.", &user);
								return Err(DbError::AuthError);
							}
							if *meta != meta_new {
								error!("User {} can't edit the metadata of chunk {}.", &user, chunk.id);
								return Err(DbError::AuthError);
							}
						}

						let mut users = HashSet::default();
						users.insert(chunk.owner.clone());
						users.extend(meta_new.access.iter().map(|(u, _)| u.clone()));


						let users_access_changed = meta_new
							.access
							.symmetric_difference(&meta.access)
							.map(|(u, _)| u.clone())
							.collect::<HashSet<_>>();

						chunk.modified = get_secs();
						chunk.value = value;
						*meta = meta_new;

						Ok((chunk.clone(), users, users_access_changed))
					}
					None => {
						println!("Chunk '{}' not found", &id);
						return Err(DbError::AuthError);
					}
				}
			}
			None => {
				// User wants to create a chunk

				// Generate non-colliding id
				let mut id = gen_proquint();
				while self.chunks.contains_key(&id) {
					id = gen_proquint();
				}

				// Create new chunk
				let chunk = Chunk::new(id.clone(), value, user.into())?;

				let users = HashSet::from_iter(
					meta_new
						.access
						.iter()
						.map(|(u, _)| u.clone())
						.chain([chunk.owner.clone()].into_iter()),
				);

				self.chunks.insert(id, (chunk.clone(), meta_new));

				// Respond
				Ok((chunk, users, HashSet::from([user.into()])))
			}
		}
	}

	pub fn del_chunk(&mut self, user: &String, ids: Vec<String>) -> Result<Vec<(Chunk, ChunkMeta)>, DbError> {
		let mut chunks_changed = vec![];

		// Check everything is good
		for id in ids.clone() {
			if let Some((chunk, meta)) = self.chunks.get(&id) {
				if *user != chunk.owner && !meta.access.contains(&(user.clone(), Access::Read)) {
					error!("Some chunks not owner/read access by '{}' : '{:?}'.", user, ids);
					return Err(DbError::AuthError);
				}
			} else {
				error!("Some ids not found '{:?}'.", ids);
				return Err(DbError::NotFound);
			}
		}
		for id in ids {
			let mut should_remove = false;
			{
				let (chunk, meta) = self.chunks.get_mut(&id).unwrap();
				if chunk.owner != *user {
					meta.access.remove(&(user.clone(), Access::Read));
					meta.access.remove(&(user.clone(), Access::Write));
					chunk.value = meta.to_string(&chunk.value);
					// Nofity users of chunk change
					chunks_changed.push((chunk.clone(), meta.clone()));
				} else {
					should_remove = true;
				}
			}
			if should_remove {
				self.chunks.remove(&id);
			}
		}

		Ok(chunks_changed)
	}
}


#[cfg(test)]
mod tests {
	use super::*;
	// use log::{info};
	fn init() -> DB {
		let mut db = DB::default();
		assert!(db.new_user("nina".into(), "4444".into()).is_ok());
		assert!(db.new_user("john".into(), "3333".into()).is_ok());

		assert!(db.set_chunk("nina", (None, "# Todo".into())).is_ok());
		assert!(db
			.set_chunk("nina", (None, "# Chores -> Todo\n - Vaccum\naccess: john r".into()))
			.is_ok());

		assert!(db.set_chunk("john", (None, "# Todo".into())).is_ok());
		assert!(db.set_chunk("john", (None, "# Groceries -> todo".into())).is_ok());
		assert!(db
			.set_chunk("john", (None, "# Work Stuff -> todo\nshare: nina write".into()))
			.is_ok());

		db
	}
	#[test]
	fn users() {
		let mut db = DB::default();
		assert_eq!(db.new_user("Nana3".into(), "1234".into()), Err(DbError::InvalidUser));
		assert_eq!(db.new_user("Nana&".into(), "1234".into()), Err(DbError::InvalidUser));
		assert_eq!(db.new_user(":nana".into(), "1234".into()), Err(DbError::InvalidUser));
		assert!(db.new_user("nina".into(), "nina's pass".into()).is_ok());

		assert_eq!(db.users.len(), 1);

		assert!(db.login("nina", "wrong_pass").is_err());
		assert!(db.login("nana", "wrong_pass").is_err());
		assert!(db.login("nina", "nina's pass").is_ok());
	}

	#[test]
	fn chunks() {
		let mut db = init();
		// Checking chunk validation
		assert!(db.set_chunk("nina", (None, "4444".into())).is_err());
		assert!(db.set_chunk("nina", (None, "# -> jack".into())).is_err());
		assert!(db.set_chunk("nina", (None, "#nack".into())).is_err());
		assert!(db.set_chunk("nina", (None, "access: nomad read".into())).is_err());


		let nina_chores = db.get_chunk(Some("nina".into()), &"Chores".into()).unwrap();
		let john_work_stuff = db.get_chunk(Some("john".into()), &"Work Stuff".into()).unwrap();

		assert_eq!(db.get_notes("nina").len(), 3);
		assert_eq!(db.get_notes("john").len(), 4);

		assert!(db
			.set_chunk(
				"john",
				(
					Some(nina_chores.id.clone()),
					"# Chores -> Todo\n - Vaccum\naccess: john r".into()
				)
			)
			.is_err());
		assert!(db
			.set_chunk(
				"john",
				(Some(nina_chores.id.clone()), "# Chores -> Todo\n - Vaccum".into())
			)
			.is_err());


		assert!(
			db.set_chunk(
				"nina",
				(
					Some(john_work_stuff.id.clone()),
					"# Work Stu -> todo\nshare: nina write".into()
				)
			)
			.is_err(),
			"Nina has write access but can't change title, title is checked by _ref/title props in ChunkMeta"
		);
		let r = db.set_chunk(
			"nina",
			(
				Some(john_work_stuff.id.clone()),
				"# work stuff -> Todo\nshare: nina w".into(),
			),
		);
		assert!(r.is_err(), "Title Changed, write should fail'{r:?}'");
		let r = db.set_chunk(
			"nina",
			(
				Some(john_work_stuff.id.clone()),
				"# Work Stuff -> Todo\nshare: nina r".into(),
			),
		);
		assert!(r.is_err(), "Can't change access, fails'{r:?}'");
		let r = db.set_chunk(
			"nina",
			(
				Some(john_work_stuff.id.clone()),
				"# Work Stuff -> Todo\nCan change content :)\nshare: nina w".into(),
			),
		);
		assert!(
			r.is_ok(),
			"Can change content since nina has write access, succeeds'{r:?}'"
		);
	}
	#[test]
	fn views() {
		// let db = init();
	}
	#[test]
	fn delete() {
		let mut db = init();

		let john_work_stuff = db.get_chunk(Some("john".into()), &"Work Stuff".into()).unwrap();
		assert!(db.del_chunk(&"nina".into(), vec![john_work_stuff.id.clone()]).is_err());
	}
	#[test]
	fn access() {}
}
