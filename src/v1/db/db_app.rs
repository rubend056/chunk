use super::{db_auth::DBAuth, db_chunk::DBChunk, Access, DBData, GraphView, UserAccess, DB};
use crate::utils::{diff_calc, DbError};

/**
 * A DB without a reference (normalized title) implementation and actual dynamic memory pointers instead of repetitive lookups.
 * Should be orders of magnitud simpler and faster.
 */
use log::error;
use serde_json::{json, Value};
use std::{
	collections::HashSet,
	sync::{Arc, RwLock},
};

impl DB {
	/// Goes through tree and creates a GraphView
	///
	/// - If root=None, iter=0 --> [Value::Null ] // We pull nothing
	/// - If root=None, iter=1 --> [Value::Null, [ ["basab"] ] ] // We pull 1rst level
	/// - If root=None, iter=2 --> [Value::Null, [ ["basab", [["basorb"]] ] ] ] // We pull 2nd level
	pub fn subtree<CF, VF>(
		&self,
		root: Option<&Arc<RwLock<DBChunk>>>,
		ua: &UserAccess,
		// Function that modifies children, perhaps to sort them
		children_fn: &CF,
		// Function that turns Node -> View
		view_fn: &VF,
		iter: i32,
	) -> GraphView
	where
		CF: Fn(Vec<Arc<RwLock<DBChunk>>>) -> Vec<Arc<RwLock<DBChunk>>>,
		VF: Fn(&Arc<RwLock<DBChunk>>) -> Value,
	{
		// public assertion
		if ua.user == "public" {
			return GraphView(Value::Null, None);
		}

		if let Some(chunk) = root {
			GraphView(
				view_fn(chunk),
				if iter > 0 {
					let c = chunk.read().unwrap().children(Some(ua));
					Some(
						children_fn(c)
							.into_iter()
							.map(|root| self.subtree(Some(&root), ua, children_fn, view_fn, iter - 1))
							.collect(),
					)
				} else {
					None
				},
			)
		} else {
			GraphView(
				Value::Null,
				if iter > 0 {
					Some(
						children_fn(
							self
								.chunks
								.values()
								.filter_map(|v| {
									let mut g = false;
									{
										if let Ok(chunk) = v.read() {
											if chunk.has_access(ua) && chunk.parents(Some(ua)).is_empty() {
												g = true;
											}
										}
									}
									if g {
										Some(v.to_owned())
									} else {
										None
									}
								})
								.collect(),
						)
						.into_iter()
						.map(|root| self.subtree(Some(&root), ua, children_fn, view_fn, iter - 1))
						.collect(),
					)
				} else {
					None
				},
			)
		}
	}

	pub fn get_chunks(&mut self, user: &str) -> Vec<Arc<RwLock<DBChunk>>> {
		// public assertion
		if user == "public" {
			return vec![];
		}

		self
			.chunks
			.values()
			.filter_map(|v| {
				if let Ok(chunk) = v.write() {
					if chunk.has_access(&user.into()) {
						return Some(v.clone());
					}
				}
				None
			})
			.collect()
	}

	///  Gets a chunk by id
	pub fn get_chunk(&self, id: &str, user: &str) -> Option<Arc<RwLock<DBChunk>>> {
		self.chunks.get(id).and_then(|chunk_ref| {
			let chunk = chunk_ref.write().unwrap();
			if chunk.has_access(&user.into()) || chunk.is_public() {
				Some(chunk_ref.clone())
			} else {
				None
			}
		})
	}

	/// Deletes a chunk by id, returns list of users for which access changed

	pub fn del_chunk(&mut self, ids: HashSet<String>, user: &str) -> Result<HashSet<String>, DbError> {
		// public assertion
		if user == "public" {
			error!("Public tried to delete {:?}", &ids);
			return Err(DbError::AuthError);
		}

		let mut changed = HashSet::<String>::default();
		let mut to_remove = HashSet::<String>::default();

		for id in ids {
			// Temporary variables for update
			let mut chunk_to_replace = None;
			if let Some(chunk_ref) = self.chunks.get(&id) {
				let chunk = chunk_ref.write().unwrap();
				if chunk.has_access(&(user.to_owned(), Access::Admin).into()) {
					to_remove.insert(chunk.chunk().id.to_owned());
					changed.extend(chunk.access_diff(None));
				} else if chunk.has_access(&user.into()) {
					// Have to think about this a bit more, specially when concerning groups
					// If a user has read access and he/she is part of a group there has to be a way for them to exit out...
					let mut chunk = DBChunk::from((id.as_str(), chunk.chunk().value.as_str(), chunk.chunk().owner.as_str()));
					let mut access = chunk
						.get_prop::<HashSet<UserAccess>>("access")
						.expect("If user has read access, access has to be valid here");
					access.retain(|ua| ua.user != user); // Remove all of this users's access
					if !chunk.r#override("access", json!(access)) {
						error!("Couldn't do shit here");
						return Err(DbError::AuthError);
					};
					chunk_to_replace = Some(chunk);
				} else {
					return Err(DbError::AuthError);
				}
			} else {
				return Err(DbError::NotFound);
			}
			// Perform the update
			if let Some(chunk_to_replace) = chunk_to_replace {
				let owner = chunk_to_replace.chunk().owner.clone();
				self.set_chunk(chunk_to_replace, owner.as_str()).unwrap();

				changed.insert(user.into());
			}
		}

		// Delete all them chunks which have to be deleted
		to_remove.iter().for_each(|id| {
			{
				// Invalidate all parents
				self.chunks.get(id).unwrap().write().unwrap().invalidate(&vec![], true)
			}
			self.chunks.remove(id);
		});

		Ok(changed)
	}
	/// Receives a Chunk which it validates & links, returns the list of users for which access changed
	///
	pub fn set_chunk(&mut self, mut chunk: DBChunk, user: &str) -> Result<HashSet<String>, DbError> {
		// public assertion
		if user == "public" {
			error!("Public can't set/modify a chunk.");
			return Err(DbError::AuthError);
		}

		let diff_users;
		let diff_props;
		if let Some(chunk_old) = self.chunks.get(&chunk.chunk().id).cloned() {
			// Updating
			let chunk_old = chunk_old.write().unwrap();

			// Perform update check
			if !chunk_old.try_clone_to(&mut chunk, user) {
				return Err(DbError::AuthError);
			}

			// Find diff, link and insert
			diff_users = chunk_old.access_diff(Some(&chunk));
			diff_props = chunk_old.props_diff(Some(&chunk));
		} else {
			// Creating
			// If creating a chunk, user has to be same as Chunk owner
			chunk.set_owner(user.to_owned());

			// Find diff, link and insert
			diff_users = chunk.access_diff(None);
			diff_props = chunk.props_diff(None);
		}

		let id = chunk.chunk().id.clone();
		let chunk = Arc::new(RwLock::new(chunk));
		self.link_chunk(&chunk, None)?;
		{
			let mut chunk = chunk.write().unwrap();
			chunk.invalidate(&vec!["modified"], true);
		}

		self.chunks.insert(id, chunk);

		Ok(diff_users)
	}
	/// Chunk update called by socket, adds `diff` information to returned Result
	pub fn update_chunk(
		&mut self,
		chunk: DBChunk,
		user: &str,
	) -> Result<(HashSet<String>, Vec<String>, Arc<RwLock<DBChunk>>), DbError> {
		if let Some(last_value) = self
			.get_chunk(&chunk.chunk().id, user)
			.map(|v| v.read().unwrap().chunk().value.to_owned())
		{
			let value = chunk.chunk().value.clone();
			let id = chunk.chunk().id.clone();
			let users_to_notify = self.set_chunk(chunk, user)?;
			let diff = diff_calc(&last_value, &value);
			let db_chunk = self.get_chunk(&id, user).unwrap();
			return Ok((users_to_notify, diff, db_chunk));
		}
		Err(DbError::NotFound)
	}
	pub fn link_all(&mut self) -> Result<(), DbError> {
		let chunks = self.chunks.values().cloned().collect::<Vec<_>>();
		for chunk in chunks {
			self.link_chunk(&chunk, None)?;
		}
		Ok(())
	}

	/// Processes a chunk within the tree. Making sure there are no circular references.
	/// Recursively calls itself for every parent found
	///
	/// Description.
	///
	/// * `chunk` - The chunk that's currently being linked
	/// * `child` - If None, `chunk` is the original, Some if its a recursive iteration and we're checking for circulars.
	fn link_chunk(&mut self, chunk: &Arc<RwLock<DBChunk>>, child: Option<&Arc<RwLock<DBChunk>>>) -> Result<(), DbError> {
		// Detect circular reference
		if let Some(child) = child {
			// If child was Some, means this is a recursive iteration
			if Arc::ptr_eq(chunk, child) {
				println!("Circular reference detected!");
				return Err(DbError::InvalidChunk);
			}
		}

		// Link parents and tell parents about us if we haven't already
		{
			let mut chunk_lock = chunk.try_write().unwrap();
			if !chunk_lock.linked {
				// Link parents by matching ids to existing chunks
				if let Some(parent_ids) = chunk_lock.get_prop::<Vec<String>>("parents") {
					if parent_ids.contains(&chunk_lock.chunk().id) {
						error!("Circular reference detected!; Links to itself");
						return Err(DbError::InvalidChunk);
					}

					let parent_weaks = parent_ids
						.iter()
						.filter_map(|id| self.chunks.get(id).map(Arc::downgrade));

					chunk_lock.parents.extend(parent_weaks);
				}
				// Tell those parents that this is one of their children
				chunk_lock.parents(None).iter().for_each(|v| {
					if let Ok(mut v) = v.write() {
						v.link_child(chunk);
					}
				});
				// Tell those children that this is one of their parents
				chunk_lock.children(None).iter().for_each(|v| {
					if let Ok(mut v) = v.write() {
						v.link_parent(chunk);
					}
				});

				chunk_lock.linked = true;
			}
		}

		// Keep detecting any circular reference, by recursing all parents
		{
			let parents = chunk.read().unwrap().parents(None);
			for parent in parents {
				// Iterate through all parents, linking + checking for circularity
				let child = child.unwrap_or(chunk);
				// println!("Iterate chunk {} child {:?}", parent.read().unwrap().chunk().id, Arc::as_ptr(child));
				self.link_chunk(&parent, Some(child))?;
			}
		}

		Ok(())
	}
}

/**
 * Creates a base implementation of RAM data from what was saved
 */
impl From<DBData> for DB {
	fn from(data: DBData) -> Self {
		let mut db = Self {
			chunks: data
				.chunks
				.into_iter()
				.map(|c| (c.id.clone(), Arc::new(RwLock::new(DBChunk::from(c)))))
				.collect(),
			auth: DBAuth {
				users: data
					.users
					.into_iter()
					.map(|u| (u.user.clone(), Arc::new(RwLock::new(u))))
					.collect(),
				..Default::default()
			},
		};
		db.link_all().unwrap();
		db
	}
}
/**
 * From a reference because we're saving backups all the time, and it's easier to clone the underlying data
 */
impl From<&DB> for DBData {
	fn from(db: &DB) -> Self {
		DBData {
			chunks: db.chunks.values().map(|v| v.read().unwrap().chunk().clone()).collect(),
			users: db.auth.users.values().map(|v| v.read().unwrap().clone()).collect(),
			groups: db
				.auth
				.groups
				.iter()
				.map(|(group, users)| {
					(
						group.clone(),
						users
							.iter()
							.map(|u| u.upgrade().unwrap().read().unwrap().user.clone())
							.collect(),
					)
				})
				.collect(),
		}
	}
}

#[cfg(test)]
mod tests {

	use std::collections::HashSet;

	use serde_json::{json, Value};

	use crate::v1::db::{db_chunk::DBChunk, Chunk, ChunkId, ChunkView, GraphView, ViewType};

	use super::DB;
	#[test]
	fn delete() {
		let mut db = DB::default();

		let c_notes: DBChunk = "# Notes\n".into();
		let id_notes = c_notes.chunk().id.clone();
		assert!(db.set_chunk(c_notes, "john").is_ok());
		assert_eq!(
			db.del_chunk([id_notes].into(), "john"),
			Ok(HashSet::from(["john".into()]))
		);
	}
	#[test]
	fn sharing() {
		let mut db = DB::default();

		let c_notes: DBChunk = "# Notes\nshare: poca w, nina a".into();
		println!("{:?}", c_notes.props());
		let id_notes = c_notes.chunk().id.clone();
		assert!(db.set_chunk(c_notes, "john").is_ok());

		assert_eq!(
			db.set_chunk(
				(id_notes.as_str(), "# Notes\nHello :)\nshare: poca w, nina a").into(),
				"poca"
			),
			Ok(HashSet::default())
		);
		assert!(db
			.set_chunk((id_notes.as_str(), "# Notes\nshare: poca w, nina r").into(), "poca")
			.is_err());
		// let c_notes: DBChunk = "# Notes\nHello :)\nshare: poca w, nina a".into();
		// println!("{:?}", c_notes.props());
		assert!(db
			.set_chunk(
				(id_notes.as_str(), "# Notes\nHello :)\nshare: poca w, nina a").into(),
				"nina"
			)
			.is_ok());
		assert!(db
			.set_chunk((id_notes.as_str(), "# Notes\nHello :)\nshare: nina a").into(), "nina")
			.is_ok());
		assert!(db
			.set_chunk(
				(id_notes.as_str(), "# Notes\nHello :)\nshare: poca rnina a").into(),
				"nina"
			)
			.is_err()); // Errors out because nina would be deleting her own admin access
		assert!(db
			.set_chunk(
				(id_notes.as_str(), "# Notes\nHello :)\nshare: poca r,nina a").into(),
				"nina"
			)
			.is_ok());
		assert!(db.del_chunk(HashSet::from([id_notes]), "nina").is_ok()); // Nina can delete as well
	}
	// Make sure users can't see public documents in their views
	#[test]
	fn visibility() {
		let mut db = DB::default();

		// John creates a chunk
		let c_notes: DBChunk = "# Notes\nshare: public a, public r".into();
		let id_notes = c_notes.chunk().id.clone();
		assert!(db.set_chunk(c_notes, "john").is_ok());
		{
			// Test visibility

			assert_eq!(db.get_chunks("nina").len(), 0); // Nina can't see anything
			assert_eq!(db.get_chunks("john").len(), 1); // John can see his own
			assert_eq!(db.get_chunks("public").len(), 0); // Public can't see anything

			assert_eq!(
				db.subtree(None, &"nina".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, Some(vec![]))
			); // Nina can't see anything
			assert_eq!(
				db.subtree(None, &"john".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, Some(vec![GraphView(json!(id_notes), None)]))
			); // John can see his own
			assert_eq!(
				db.subtree(None, &"public".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, None)
			); // Public can't see anything
		}

		// Public tries creating a chunk but is denied
		let c_notes: DBChunk = "# Notes 2\nshare: public a, john a".into();
		assert!(db.set_chunk(c_notes, "public").is_err());
		// Nina creates a chunk, giving john admin access
		let c_notes: DBChunk = "# Notes 2\nshare: public a, john a".into();
		let id_notes2 = c_notes.chunk().id.clone();
		assert!(db.set_chunk(c_notes, "nina").is_ok());

		{
			// Test visibility

			assert_eq!(db.get_chunks("nina").len(), 1);
			assert_eq!(db.get_chunks("john").len(), 2);
			assert_eq!(db.get_chunks("public").len(), 0); // Public can't see anything

			assert_eq!(
				db.subtree(None, &"nina".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, Some(vec![GraphView(json!(id_notes2), None)]))
			);
			assert_eq!(
				db.subtree(None, &"john".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1)
					.1
					.unwrap()
					.len(),
				2
			);
			assert_eq!(
				db.subtree(None, &"public".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, None)
			); // Public can't see anything
		}

		// Nina creates another chunk, giving john also admin access
		let c_notes: DBChunk = format!("# Notes 3 -> {id_notes2}\nshare: public a, john a")
			.as_str()
			.into();
		let id_notes3 = c_notes.chunk().id.clone();
		assert!(db.set_chunk(c_notes, "nina").is_ok());

		{
			// Test visibility

			assert_eq!(db.get_chunks("nina").len(), 2);
			assert_eq!(db.get_chunks("john").len(), 3);
			assert_eq!(db.get_chunks("public").len(), 0); // Public can't see anything

			assert_eq!(
				db.subtree(None, &"nina".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, Some(vec![GraphView(json!(id_notes2), None)]))
			);
			assert_eq!(
				db.subtree(None, &"nina".into(), &|v| v, &|v| json!(ChunkId::from(v)), 2),
				GraphView(
					Value::Null,
					Some(vec![GraphView(
						json!(id_notes2),
						Some(vec![GraphView(json!(id_notes3), None)])
					)])
				)
			);
			assert_eq!(
				db.subtree(None, &"john".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1)
					.1
					.unwrap()
					.len(),
				2
			);
			assert_eq!(
				db.subtree(None, &"public".into(), &|v| v, &|v| json!(ChunkId::from(v)), 1),
				GraphView(Value::Null, None)
			); // Public can't see anything
		}
	}
	/// Create a "Notes"
	/// Modify "Notes" 10 sec after
	/// Assert that Modify is 10 sec after Created.
	#[test]
	fn created_modified() {
		let mut db = DB::default();

		let c_notes: DBChunk = "# Notes\n".into();
		let cre_notes = c_notes.chunk().created;

		let id_notes = c_notes.chunk().id.clone();
		db.set_chunk(c_notes, "john").unwrap();

		let mut c_notes: Chunk = (id_notes.as_str(), "# Notes\n").into();
		c_notes.created += 10;
		c_notes.modified += 10;
		let mod_notes = c_notes.modified;
		db.set_chunk(c_notes.into(), "john").unwrap();

		let notes = db.get_chunk(&id_notes, "john").unwrap();
		{
			let chunk_notes = notes.read().unwrap();
			assert_eq!(chunk_notes.chunk().created, cre_notes);
			assert_eq!(chunk_notes.chunk().modified, mod_notes);
		}

		{
			let view = ChunkView::from((notes, "john", ViewType::Edit));
			assert_eq!(view.created, Some(cre_notes));
			assert_eq!(view.modified, Some(mod_notes));
		}
	}
	/// Create a "Notes"
	/// Create a "Note1 -> Notes" with modified 10 sec after
	/// Assert that Dynamic Modified on Notes = Note1's modify time (10 sec after)
	#[test]
	fn dynamic_modified() {
		let mut db = DB::default();
		let c_notes: DBChunk = "# Notes\n".into();
		let mod_notes = c_notes.chunk().modified;
		let id_notes = c_notes.chunk().id.clone();
		db.set_chunk(c_notes, "john").unwrap();

		let mut chunk_note1: Chunk = format!("# Note 1 -> {}\n", &id_notes).as_str().into();
		let mod_note1 = mod_notes + 10;
		chunk_note1.modified = mod_note1;
		let c_note1 = DBChunk::from(chunk_note1);
		let _id_note1 = c_note1.chunk().id.clone();

		assert!(db.set_chunk(c_note1, "john").is_ok());

		assert_eq!(
			db.get_chunk(&id_notes, "john")
				.unwrap()
				.write()
				.unwrap()
				.get_prop_dynamic::<u64>("modified", &"john".into())
				.unwrap(),
			mod_note1
		);
	}
	#[test]
	fn well() {
		let mut db = DB::default();

		let c_notes: DBChunk = "# Notes\n".into();
		let id_notes = c_notes.chunk().id.clone();
		assert_eq!(
			db.set_chunk(c_notes, "john"),
			Ok(HashSet::from(["john".into()])),
			"users_to_notify should be 1 'john'"
		);

		let c_note1 = DBChunk::from(format!("# Note 1 -> {}\n", &id_notes).as_str());
		let _id_note1 = c_note1.chunk().id.clone();
		assert!(db.set_chunk(c_note1, "john").is_ok());

		let _all: Vec<ChunkView> = db
			.get_chunks("john")
			.into_iter()
			.map(|v| ChunkView::from((v, "john")))
			.collect();

		let subtree = db.subtree(None, &"john".into(), &|v| v, &|v| json!(ChunkId::from(v)), 2);
		// println!("{subtree:?}");
		assert_eq!(
			subtree.1.unwrap().len(),
			1,
			"Children should be 1 as john has 1 chunk without parents"
		);

		let subtree = db.subtree(
			db.get_chunk(id_notes.as_str(), "john").as_ref(),
			&"john".into(),
			&|v| v,
			&|v| json!(ChunkId::from(v)),
			2,
		);
		// println!("{subtree:?}");
		assert_eq!(
			subtree.1.unwrap().len(),
			1,
			"Children should be 1 as x has 1 chunk without parents"
		);
	}
	#[test]
	fn circular() {
		let mut db = DB::default();

		let c_notes: DBChunk = "# Notes\n".into();
		let id_notes = c_notes.chunk().id.clone();
		// Add '# Notes\n' john
		assert!(db.set_chunk(c_notes, "john").is_ok());

		let c_note1 = DBChunk::from(format!("# Note 1 -> {}\n", &id_notes).as_str());
		let id_note1 = c_note1.chunk().id.clone();
		assert!(db.set_chunk(c_note1, "john").is_ok());

		assert!(
			db.set_chunk((&*id_notes, &*format!("# Notes -> {}\n", &id_notes)).into(), "john")
				.is_err(),
			"Chunk links to itself, A -> A, it should fail."
		);
		assert!(
			db.set_chunk((&*id_notes, &*format!("# Notes -> {}\n", &id_note1)).into(), "john")
				.is_err(),
			"Chunk links circurlarly, A -> B -> A, it should fail."
		);

		let c_note2 = DBChunk::from(format!("# Note 2 -> {}\n", &id_note1).as_str());
		let id_note2 = c_note2.chunk().id.clone();
		assert!(db.set_chunk(c_note2, "john").is_ok());

		assert!(
			db.set_chunk((&*id_notes, &*format!("# Notes -> {}\n", &id_note2)).into(), "sara")
				.is_err(),
			"Chunk links circurlarly, A -> C -> B -> A, it should fail."
		);
	}
}
