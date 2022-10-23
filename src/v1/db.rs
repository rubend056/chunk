/** Designing a new Data Structure that would allow for all queries/insertions/serializations to efficiently happen */

/**
 - Different visualization options
 - Editing
	 - ![](web/src/assets/icons/card-text.svg) **Shank/Edit** -> selected chunk + children up to 4N (1N default) an editor
 - Viewing
	 - ![](web/src/assets/icons/clipboard.svg) **Notes** -> chunks ordered by recent side by side
	 - ![](web/src/assets/icons/grid.svg) **Labyrinth** -> selected chunk children on a grid
	 - ![](web/src/assets/icons/diagram-2-fill.svg) **Graph** -> nodes in a tree
				 (S)	-> (R r)
				(S) -> ()

				(R) -> (S w) ->
								\> (S r)
		Querying this with different views

*/
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
	utils::{gen_proquint, get_secs},
	v1::{
		chunk::{Chunk, ChunkMeta},
		user::User,
	},
};

use super::chunk::{Access, UserAccess, standardize};

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

#[derive(Default, Debug)]
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
				.map(|chunk| {
					(
						chunk.id.clone(),
						(chunk.clone(), ChunkMeta::from(&chunk.value)),
					)
				})
				.collect(),
			users: value
				.users
				.into_iter()
				.map(|user| (user.user.clone(), user))
				.collect(),
		}
	}
}

impl DB {
	pub fn new_user(&mut self, user: String, pass: String) -> Result<(), String> {
		if self.users.get(&user).is_some() {return Err("User taken".to_string());}
		
		let user = User::new(user.clone(), pass)?;
		
		if user.user == "public" {return Err("User not valid".to_string());}
		
		self.users.insert(user.user.clone(), user);
		Ok(())
		
	}
	pub fn login(&self, user: String, pass: String) -> Result<(), String> {
		let user = self.users.get(&user).ok_or("Login error".to_string())?;
		if !user.verify(&pass) {return Err("Login error".to_string());}
		Ok(())
	}

	fn iter_tree(
		&self,
		ua: &UserAccess,
		depth: u32,
		root: Option<(&String, &String)>,
	) -> Vec<ChunkTree> {
		self
			.chunks
			.iter()
			.filter(|(_, (chunk, meta))| {
				(meta.access.contains(ua) || chunk.owner == ua.0)
					&& match root {
						Some(root) => {
							meta._refs.contains(&(Some(root.0.clone()), root.1.clone()))
								|| (if chunk.owner == *root.0 {
									meta._refs.contains(&(None, root.1.clone()))
								} else {
									false
								})
						}
						None => meta._refs.is_empty(),
					}
			})
			.map(|(_, (chunk, meta))| {
				ChunkTree(
					chunk.clone(),
					if depth > 0 {
						Some(self.iter_tree(ua, depth - 1, Some((&chunk.owner, &meta._ref))))
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
	pub fn get_chunks(&self, user: String, depth: Option<u32>) -> Vec<ChunkTree> {
		self.iter_tree(&(user, Access::READ), depth.unwrap_or(0), None)
	}
	pub fn get_notes(&self, user: String) -> Vec<ChunkView> {
		self
			.chunks
			.iter()
			.fold(vec![], |mut acc, (_, (chunk, meta))| {
				if chunk.owner == user {
					acc.push((chunk.clone(), ChunkType::Owner));
				};
				if meta.access.contains(&(user.clone(), Access::READ)) {
					acc.push((
						chunk.clone(),
						ChunkType::Access(if meta.access.contains(&(user.clone(), Access::WRITE)) {
							Access::WRITE
						} else {
							Access::READ
						}),
					));
				};
				acc
			})
	}

	pub fn get_chunk(&self, user: Option<String>, id_or_ref: &String) -> Result<Chunk, String> {
		let user = user.unwrap_or("public".to_string());
		let id_or_ref = standardize(id_or_ref);

		if let Some((chunk, meta)) = self.chunks.get(&id_or_ref).or_else(|| {
			self
				.chunks
				.iter()
				.find(|(_, (_, meta))| meta._ref == id_or_ref)
				.map(|v| v.1)
		}) {
			if chunk.owner == user || meta.access.contains(&(user, Access::READ)) {
				return Ok(chunk.clone());
			}
		}
		Err(format!("Chunk '{}' not found", id_or_ref))
	}

	pub fn set_chunk(
		&mut self,
		user: String,
		(id, value): (Option<String>, String),
	) -> Result<Chunk, String> {
		let meta_new = ChunkMeta::from(&value);
		if meta_new._ref.is_empty() {return Err("Title required".to_string())}
		
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
							if !meta.access.contains(&(user, Access::WRITE)) {
								return Err("You don't have write access.".to_string());
							}
							if *meta != meta_new {
								return Err("You can only edit the body.".to_string());
							}
						}

						chunk.modified = get_secs();
						chunk.value = value;

						Ok(chunk.clone())
					}
					None => {
						return Err(format!("Chunk '{}' not found", &id));
					}
				}
			}
			None => {
				// User wants to create a chunk

				let mut id = gen_proquint();
				while self.chunks.contains_key(&id) {
					id = gen_proquint();
				}
				
				let chunk = Chunk::new(id.clone(), value, user)?;
				self.chunks.insert(id, (chunk.clone(), meta_new));

				Ok(chunk)
			}
		}
	}

	pub fn del_chunk(&mut self, user: &String, ids: ChunkDel) -> Result<(), String> {
		let ids = match ids {
			TOrTs::Single(id) => vec![id],
			TOrTs::Multi(ids) => ids,
		};

		// Check everything is good
		for id in &ids {
			if let Some((chunk, _)) = self.chunks.get(id) {
				if user != &chunk.owner {
					return Err(format!("You're not the owner of {id}"));
				}
			} else {
				return Err("Not all ids found".to_string());
			}
		}

		// Delete
		for id in &ids {
			self.chunks.remove(id);
		}
		Ok(())
	}
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum TOrTs<T> {
	Single(T),
	Multi(Vec<T>),
}
pub type ChunkDel = TOrTs<String>;


#[cfg(test)]
mod tests {
	use super::*;
	// use log::{info};
	fn init() -> DB {
		let mut db = DB::default();
		assert_eq!(db.new_user("nina".to_string(), "4444".to_string()), Ok(()));
		assert_eq!(db.new_user("john".to_string(), "3333".to_string()), Ok(()));
		
		assert!(db.set_chunk("nina".to_string(), (None,"# Todo".to_string())).is_ok());
		assert!(db.set_chunk("nina".to_string(), (None,"# Chores -> Todo\n - Vaccum\naccess: john r".to_string())).is_ok());
		
		assert!(db.set_chunk("john".to_string(), (None,"# Todo".to_string())).is_ok());
		assert!(db.set_chunk("john".to_string(), (None,"# Groceries -> todo".to_string())).is_ok());
		assert!(db.set_chunk("john".to_string(), (None,"# Work Stuff -> todo\nshare: nina write".to_string())).is_ok());
		
		db
	}
	#[test]
	fn users() {
		let mut db = DB::default();
		assert_eq!(db.new_user("Nana3".to_string(), "1234".to_string()), Err("User not valid".to_string()));
		assert_eq!(db.new_user("Nana&".to_string(), "1234".to_string()), Err("User not valid".to_string()));
		assert_eq!(db.new_user(":nana".to_string(), "1234".to_string()), Err("User not valid".to_string()));
		assert_eq!(db.new_user("nina".to_string(), "nina's pass".to_string()), Ok(()));
		
		assert_eq!(db.users.len(), 1);
		
		assert!(db.login("nina".to_string(), "wrong_pass".to_string()).is_err());
		assert!(db.login("nana".to_string(), "wrong_pass".to_string()).is_err());
		assert!(db.login("nina".to_string(), "nina's pass".to_string()).is_ok());
	}
	
	#[test]
	fn chunks() {
		let mut db = init();
		assert!(db.set_chunk("nina".to_string(), (None,"4444".to_string())).is_err());
		assert!(db.set_chunk("nina".to_string(), (None,"# -> jack".to_string())).is_err());
		assert!(db.set_chunk("nina".to_string(), (None,"#nack".to_string())).is_err());
		assert!(db.set_chunk("nina".to_string(), (None,"access: nomad read".to_string())).is_err());
		
		
		
		let nina_chores = db.get_chunk(Some("nina".to_string()), &"Chores".to_string()).unwrap();
		let john_work_stuff = db.get_chunk(Some("john".to_string()), &"Work Stuff".to_string()).unwrap();
		
		assert_eq!(db.get_notes("nina".to_string()).len(), 3);
		assert_eq!(db.get_notes("john".to_string()).len(), 4);
		
		assert!(db.set_chunk("john".to_string(), (Some(nina_chores.id.clone()),"# Chores -> Todo\n - Vaccum\naccess: john r".to_string())).is_err());
		assert!(db.set_chunk("john".to_string(), (Some(nina_chores.id.clone()),"# Chores -> Todo\n - Vaccum".to_string())).is_err());
		
		assert!(db.set_chunk("nina".to_string(), (Some(john_work_stuff.id.clone()),"# Work Stu -> todo\nshare: nina write".to_string())).is_err());
		assert!(db.set_chunk("nina".to_string(), (Some(john_work_stuff.id.clone()),"# work_stuff -> Todo\nshare: nina w".to_string())).is_ok());
		assert!(db.set_chunk("nina".to_string(), (Some(john_work_stuff.id.clone()),"# work_stuff -> Todo\nshare: nina r".to_string())).is_err());
		assert!(db.set_chunk("nina".to_string(), (Some(john_work_stuff.id.clone()),"# Work Stuff -> todo\nDamn I can do this\nshare: nina write".to_string())).is_ok());
		
		
	}
	#[test]
	fn views() {
		let db = init();
		
		assert!(db.get_chunks("nina".to_string(), None).len() == 1);
	}
	#[test]
	fn access() {
		
	}
}
