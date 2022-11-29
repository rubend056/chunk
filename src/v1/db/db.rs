use std::collections::{HashMap, HashSet};

use log::error;

use crate::{
	utils::{gen_proquint, get_secs, DbError},
	v1::{
		chunk::{standardize, Access, Chunk, ChunkMeta, UserAccess},
		user::User,
	},
};

use super::{ChunkAndMeta, ChunkTree, ChunkType, ChunkView, DBData, UsersToNotify, DB};

impl From<DBData> for DB {
	fn from(value: DBData) -> Self {
		let chunks = value
			.chunks
			.into_iter()
			.map(|chunk| (chunk.id.clone(), (chunk.clone(), ChunkMeta::from(&chunk.value))))
			.collect::<HashMap<String, ChunkAndMeta>>();
		// Ref->id on conversion
		let mut ref_id = HashMap::<String, Vec<String>>::default();
		chunks.iter().for_each(|(id, (_, meta))| {
			ref_id
				.entry(meta._ref.to_owned())
				.and_modify(|v| v.push(id.to_owned()))
				.or_insert(vec![id.to_owned()]);
		});

		DB {
			chunks,
			users: value.users.into_iter().map(|user| (user.user.clone(), user)).collect(),
			ref_id,
		}
	}
}

impl DB {
	pub fn new_user(&mut self, user: String, pass: String) -> Result<(), DbError> {
		if self.users.get(&user).is_some() {
			return Err(DbError::UserTaken);
		}

		let user_instance = User::new(user.clone(), pass)?;

		if user == "public" {
			return Err(DbError::InvalidUser);
		}

		self.users.insert(user.clone(), user_instance);

		{
			// New user setup
			if let Ok(chunk) = self.get_chunk(Some("rubend".into()), &"tutorial".into()) {
				self.set_chunk(&user, (None, chunk.value))?;
			}
		}

		Ok(())
	}
	// pub fn remove_user(&mut self, user: String, _pass: String) -> Result<(), DbError> {
	// 	if let Some(_user_instance) = self.users.get(&user) {
	// 		if user == "public" {
	// 			return Err(DbError::InvalidUser);
	// 		}
	// 	}else {
	// 		return Err(DbError::AuthError);
	// 	}

	// 	// ! NOT IMPLEMENTED
	// 	// self.users.insert(user.clone(), user_instance);
	// 	// {
	// 	// 	// New user setup
	// 	// 	if let Ok(chunk) = self.get_chunk(Some("rubend".into()), &"tutorial".into()){
	// 	// 		self.set_chunk(&user, (None, chunk.value))?;
	// 	// 	}
	// 	// }

	// 	Ok(())
	// }
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

	fn iter_tree(&self, user_access: &UserAccess, root: Option<(&Chunk, &ChunkMeta)>, depth: u32) -> Vec<ChunkTree> {
		self
			.chunks
			.iter()
			.filter(|(_, (chunk, meta))| {
					// Does this user have access?
					(meta.access.contains(user_access) || chunk.owner == user_access.0)
					// Are you my child?
					&& {
						// Replaces none user for chunk owner
						let _refs = meta._refs.iter().map(|v| (v.0.clone().or(Some(chunk.owner.clone())),v.1.clone())).collect::<HashSet<_>>();
						match root {
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
						None => meta._refs.is_empty()
						// Return if all this chunk points to is stuff this user can't read
						|| _refs.iter().all(|v| self.ref_id.get(&v.1).and_then(|v| Some(v.iter().all(|id| {let (c,m) = &self.chunks[id];c.owner != chunk.owner && !m.access.contains(user_access)}))).or(Some(false)).unwrap()),
					}}
			})
			.map(|(_, (chunk, meta))| {
				ChunkTree(
					chunk.clone(),
					if depth > 0 {
						Some(self.iter_tree(user_access, Some((chunk, meta)), depth - 1))
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
				// Modifying
				match self.chunks.get_mut(&id) {
					Some((chunk, meta)) => {
						// Make sure user can do what he wants
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

						// Modify chunk
						let mut users = HashSet::default();
						users.insert(chunk.owner.clone());
						users.extend(meta_new.access.iter().map(|(u, _)| u.clone()));

						let users_access_changed = meta_new
							.access
							.symmetric_difference(&meta.access)
							.map(|(u, _)| u.clone())
							.collect::<HashSet<_>>();

						// Modify _ref->id
						if meta._ref != meta_new._ref {
							let mut d = false;
							{
								self.ref_id.entry(meta._ref.clone()).and_modify(|v| {
									v.retain(|v| v != &chunk.id);
									if v.is_empty() {
										d = true;
									}
								});
								self
									.ref_id
									.entry(meta_new._ref.clone())
									.and_modify(|v| v.push(chunk.id.to_owned()))
									.or_insert(vec![chunk.id.to_owned()]);
							}
						}

						chunk.modified = get_secs();
						chunk.value = value;
						*meta = meta_new.clone();

						Ok((chunk.clone(), users, users_access_changed))
					}
					None => {
						return Err(DbError::AuthError);
					}
				}
			}
			None => {
				// Creating

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

				// Modify _ref->id
				self
					.ref_id
					.entry(meta_new._ref.clone())
					.and_modify(|v| v.push(chunk.id.to_owned()))
					.or_insert(vec![chunk.id.to_owned()]);

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

					// Modify _ref->id
					self.ref_id.remove(&meta._ref);
				}
			}
			if should_remove {
				self.chunks.remove(&id);
			}
		}

		Ok(chunks_changed)
	}
}
