use crate::utils::{standardize, REGEX_ACCESS, REGEX_PROPERTY, REGEX_TITLE, REGEX_USERNAME};
use lazy_static::lazy_static;
use log::error;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
	collections::{HashMap, HashSet},
	fmt::Debug,
	sync::{Arc, RwLock, Weak},
};

use super::{Access, Chunk, UserAccess};

/**
 * Extracted from Chunk
 */
// struct Property {
// 	key: String,
// 	value: Value,
// }
/**
 * Calculated based on Property, Parents, or Children
 */

struct DynamicProperty {
	key: String,
	function: fn(v: &mut DBChunk, others: Vec<Arc<RwLock<DBChunk>>>, &UserAccess) -> Value,
	depends_on: Option<Vec<String>>,
	/// Is the value derived from the parents?
	///
	/// - If **true**, `others` is **parents**
	/// - If **false**, `others` is **children**
	function_up: bool,
}
impl Debug for DynamicProperty {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("key {}, up {}", self.key, self.function_up))
	}
}

/**
 * What's a chunk's source of truth, is it a string, or a collection of values?
 *
 **/

/**
 * Chunk Meta will now own Chunk instead of it being a tuple in DB. Hopefully making it easier to work with
 */

#[derive(Debug)]
pub struct DBChunk {
	chunk: Chunk,
	/**
	 * Statically extracted properties
	 */
	props: HashMap<String, Value>,
	/**
	 * Additional dynamic custom properties ? Obscure idea At The Moment
	 */
	props_dynamic_custom: Vec<DynamicProperty>,
	/**
	 * Dynamic prop values defined by  (User + Key) -> Value
	 */
	props_per_user: HashMap<(String, String), Value>,
	/**
	 * parents, whoever modifies these refs, has to make sure there are no circular references
	 * */
	pub parents: Vec<Weak<RwLock<DBChunk>>>,
	/**
	 * Determines wether this chunk has been linked to parents.
	 */
	pub linked: bool,
	/**
	 * children, whoever modifies these refs, has to make sure there are no circular references
	 * */
	pub children: Vec<Weak<RwLock<DBChunk>>>,
}
impl Default for DBChunk {
	fn default() -> Self {
		Self {
			chunk: Default::default(),
			props: Default::default(),
			props_dynamic_custom: Default::default(),
			props_per_user: Default::default(),
			parents: Default::default(),
			linked: false,
			children: Default::default(),
		}
	}
}
impl<T: Into<Chunk>> From<T> for DBChunk {
	fn from(chunk: T) -> Self {
		let mut v = Self {
			chunk: chunk.into(),
			..Default::default()
		};
		v.extract();
		v
	}
}

fn access_f(v: &mut DBChunk, others: Vec<Arc<RwLock<DBChunk>>>, ua: &UserAccess) -> Value {
	// v.get_prop::<Vec<UserAccess>>("access")
	Value::Null
}
fn modified_f(v: &mut DBChunk, others: Vec<Arc<RwLock<DBChunk>>>, ua: &UserAccess) -> Value {
	let modified = others.iter().fold(v.chunk.modified, |acc, v| {
		std::cmp::max(
			v.write().unwrap().get_prop_dynamic::<u64>("modified", ua).unwrap_or(0),
			acc,
		)
	});
	json!(modified)
}

lazy_static! {
	static ref DYNAMIC_PROPS: [DynamicProperty; 1] = [
		// DynamicProperty {
		// 	key: "access".to_string(),
		// 	function: access_f,
		// 	function_up: true,
		// 	depends_on: vec!["access".into()]
		// },
		DynamicProperty {
			key: "modified".to_string(),
			function: modified_f,
			function_up: false,
			depends_on: None,
		},
	];
}

impl DBChunk {
	pub fn chunk(&self) -> &Chunk {
		return &self.chunk;
	}
	pub fn set_owner(&mut self, owner: String) {
		self.chunk.owner = owner;
	}
	/**
	 * Fills props with extracted static values
	 */
	fn extract(&mut self) {
		// Clear previous
		self.props.clear();

		// Ref + Title + Parents
		if let Some(captures) = REGEX_TITLE.captures(&self.chunk.value) {
			if let Some(m) = captures.get(1) {
				self.props.insert("title".into(), json!(String::from(m.as_str())));
				self.props.insert("ref".into(), json!(standardize(m.as_str())));
			}
			if let Some(m) = captures.get(2) {
				let parents = m
					.as_str()
					.split(",")
					.filter_map(|v| {
						let v = v.trim();
						if v.is_empty() {
							None
						} else {
							Some(v.into())
						}
					})
					.collect::<HashSet<String>>();
				self.props.insert("parents".into(), json!(parents));
			}
		}

		// Extract static properties
		for capture in REGEX_PROPERTY.captures_iter(&self.chunk.value) {
			if let Some(prop_name) = capture.get(1) {
				// Insert `<key>: <value>`, replace value with empty string if None
				self.props.insert(
					prop_name.as_str().into(),
					json!(capture.get(2).and_then(|m| Some(m.as_str()))),
				);
			}
		}

		// Extract static access
		let mut access: HashSet<UserAccess> = Default::default();
		extract_access(&self.chunk.value, &mut access);
		if !access.is_empty() {
			self.props.insert("access".to_string(), json!(access));
		}
	}
	/** Gets users with access to this note, including the owner */
	pub fn access_users(&self) -> HashSet<String> {
		let mut access: HashSet<String> = self
			.get_prop::<HashSet<UserAccess>>("access")
			.unwrap_or_default()
			.into_iter()
			.map(|v| v.user.clone())
			.collect();
		access.insert(self.chunk.owner.clone());
		access
	}
	/// Used to find out who has to be notified that access was changed for them
	///
	/// Calculates the difference in users with access between this/other chunk.
	///
	/// Other could be None if deletion/creation is happening
	pub fn access_diff(&self, other: Option<&Self>) -> HashSet<String> {
		let access = self.access_users();
		let access_other = other.and_then(|v| Some(v.access_users())).unwrap_or_default();

		access.difference(&access_other).map(|v| v.clone()).collect()
	}
	pub fn props_diff(&self, other: Option<&Self>) -> HashSet<String> {
		let mut props = self.props().into_iter().collect::<Map<String, Value>>();
		let props_other = other
			.and_then(|o| Some(o.props().into_iter().collect::<Map<String, Value>>()))
			.unwrap_or_default();
		props.retain(|k, v| props_other.get(k).and_then(|_v| Some(_v != v)).unwrap_or(true));

		props.into_iter().map(|(k, _)| k).collect()
	}
	/// Gets a static property.
	pub fn get_prop<T: for<'de> Deserialize<'de>>(&self, v: &str) -> Option<T> {
		self.props.get(v).and_then(|v| serde_json::from_value(v.clone()).ok())
	}
	pub fn props(&self) -> Vec<(String, Value)> {
		self.props.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
	}
	/// Gets a dynamic property.
	pub fn try_prop_dynamic<T: for<'de> Deserialize<'de>>(&self, user_key: &(String, String)) -> Option<T> {
		self
			.props_per_user
			.get(user_key)
			.and_then(|v| serde_json::from_value(v.clone()).ok())
	}
	pub fn props_dynamic(&mut self, ua: &UserAccess) -> Vec<(String, Value)> {
		let keys = DYNAMIC_PROPS
			.iter()
			.chain(self.props_dynamic_custom.iter())
			.map(|prop| prop.key.clone())
			.collect::<Vec<_>>();
		keys
			.into_iter()
			.filter_map(|key| {
				// let key = prop.key.clone();
				let value = self.get_prop_dynamic::<Value>(&key, ua);
				if let Some(value) = value {
					Some((key, value))
				} else {
					None
				}
			})
			.collect()
	}
	/// Gets a dynamic property.
	///
	/// If it's not present, will recalculate by calling it's corresponding function.
	pub fn get_prop_dynamic<T: for<'de> Deserialize<'de>>(&mut self, key: &str, ua: &UserAccess) -> Option<T> {
		if let Some(dprop) = self.try_prop_dynamic::<T>(&(ua.user.clone(), key.to_string())) {
			return Some(dprop);
		} else {
			let mut function = None;
			let mut up = false;

			if let Some(prop) = DYNAMIC_PROPS
				.iter()
				.chain(self.props_dynamic_custom.iter())
				.find_map(|prop| if prop.key == key { Some(prop) } else { None })
			{
				function = Some(prop.function);
				up = prop.function_up;
			}
			if let Some(function) = function {
				let others = if up {
					self.parents(Some(ua))
				} else {
					self.children(Some(ua))
				};

				let value_new = function(self, others, ua);
				self
					.props_per_user
					.insert((ua.user.clone(), key.to_string()), value_new.clone());
				return serde_json::from_value(value_new).ok();
			}
		}

		None
	}
	/**
	 * Recursive invalidator of passed `keys`
	 * up: true (recurse parents) / false (children)
	 */
	pub fn invalidate(&mut self, keys: &Vec<&str>, up: bool) {
		self.props_per_user.retain(|(_, k), _| !keys.contains(&k.as_str()));
		let others = if up { &self.parents } else { &self.children };
		others
			.iter()
			.filter_map(|v| v.upgrade())
			.for_each(|v| v.write().unwrap().invalidate(keys, up));
	}
	// pub fn invalidate_for_user(&mut self, keys: &Vec<&str>, up: bool) {
	// 	self.props_per_user.retain(|(_, k), _| !keys.contains(&k.as_str()));
	// 	let others = if up { &self.parents } else { &self.children };
	// 	others
	// 		.iter()
	// 		.filter_map(|v| v.upgrade())
	// 		.for_each(|v| v.write().unwrap().invalidate(keys, up));
	// }

	/**
	 * Checks if user has X access. Always returns true if user is the owner.
	 */
	pub fn has_access(&self, ua: &UserAccess) -> bool {
		if self.chunk.owner == ua.user {
			return true;
		}
		if let Some(access) = self.get_prop::<HashSet<UserAccess>>("access")
		// For when groups/inheritance is implemented
		// self.get_prop_dynamic::<HashSet<UserAccess>>("access", ua)
		{
			access.contains(ua)
		} else {
			false
		}
	}
	/**
	 * Returns highest access user is allowed for this chunk
	 */
	pub fn highest_access(&self, user: &str) -> Option<Access> {
		if self.chunk.owner == user {
			return Some(Access::Owner);
		}
		self.get_prop::<HashSet<UserAccess>>("access").and_then(|access| {
			let mut access: Vec<Access> = access
				.into_iter()
				.filter_map(|ua| if ua.user == user { Some(ua.access) } else { None })
				.collect();
			access.sort();
			access.reverse(); // To get highest access
			access.first().cloned()
		})
	}
	/**
	 * Function figure out if this chunk can be replaced by a new one.
	 */
	pub fn try_update(&self, other: &mut Self, user: &str) -> bool {
		if self.chunk.id != other.chunk.id.clone() {
			return false;
		}
		other.chunk.created = self.chunk.created;
		other.children = self.children.clone();

		if self.chunk.owner == user {
			if other.chunk.owner.is_empty() {
				other.chunk.owner = self.chunk.owner.clone();
			}
			return true;
		} else {
			other.chunk.owner = self.chunk.owner.clone();
		}

		if self.chunk != other.chunk {
			error!(
				"User {user} trying to change inmuttable data of {:?} that's bad!",
				&self.chunk
			);
			return false;
		}
		if let Some(access) = self.get_prop::<HashSet<(String, Access)>>("access") {
			// let ua = (user.to_string(), Access::Admin);
			if access.contains(&(user.to_string(), Access::Admin)) {
				return true;
			} else if access.contains(&(user.to_string(), Access::Write)) {
				return self.get_prop::<Value>("access") == other.get_prop("access")
					&& self.get_prop::<Value>("title") == other.get_prop("title")
					&& self.get_prop::<Value>("parents") == other.get_prop("parents");
			}
		}
		false
	}
	pub fn parents(&self, ua: Option<&UserAccess>) -> Vec<Arc<RwLock<DBChunk>>> {
		self
			.parents
			.iter()
			.filter_map(|v_weak| {
				if let Some(v_rc) = v_weak.upgrade() {
					if let Some(ua) = ua {
						if let Some(mut v) = v_rc.write().ok() {
							if !v.has_access(ua) {
								return None;
							}
						}
					} else {
						return Some(v_rc); // If ua is None
					}
					return Some(v_rc); // If ua is Some and user had access
				} else {
					None // If item has been dropped
				}
			})
			.collect()
	}
	pub fn children(&self, ua: Option<&UserAccess>) -> Vec<Arc<RwLock<DBChunk>>> {
		self
			.children
			.iter()
			.filter_map(|v_weak| {
				if let Some(v_rc) = v_weak.upgrade() {
					if let Some(ua) = ua {
						if let Some(mut v) = v_rc.write().ok() {
							if !v.has_access(ua) {
								return None;
							}
						}
					} else {
						return Some(v_rc); // If ua is None
					}
					return Some(v_rc); // If ua is Some and user had access
				} else {
					None // If item has been dropped
				}
			})
			.collect()
	}
	/**
	 * Links child and removes any dangling pointers for a self healing vector
	 */
	pub fn link_child(&mut self, child: &Arc<RwLock<DBChunk>>) {
		self.children.push(Arc::downgrade(child));
		self.children.retain(|v| v.upgrade().is_some());
	}
	/**
	 * Links child and removes any dangling pointers for a self healing vector
	 */
	pub fn link_parent(&mut self, parent: &Arc<RwLock<DBChunk>>) {
		self.parents.push(Arc::downgrade(parent));
		self.parents.retain(|v| v.upgrade().is_some());
	}
}

pub fn extract_access(value: &String, access: &mut HashSet<UserAccess>) {
	for capture in REGEX_ACCESS.captures_iter(&value) {
		if let Some(m) = capture.get(1) {
			m.as_str()
				.to_lowercase()
				.split(",")
				.filter_map(|ua| {
					let user_access = ua
						.trim()
						.split(" ")
						.filter_map(|v| {
							let o = v.trim();
							if o.is_empty() {
								None
							} else {
								Some(o)
							}
						})
						.map(|v| v.trim())
						.collect::<Vec<_>>();
					if user_access.len() < 2 {
						error!("user_access piece '{}' was parsed to length < 2?", ua);
						return None;
					}
					if !REGEX_USERNAME.is_match(user_access[0]) {
						error!("user_access user '{}' doesn't match user regex?", user_access[0]);
						return None;
					}
					let (user, access) = (user_access[0], user_access[1]);
					Some(UserAccess::from((
						user,
						if access == "r" || access == "read" {
							Access::Read
						} else if access == "w" || access == "write" {
							Access::Write
						} else if access == "a" || access == "admin" {
							Access::Admin
						} else {
							error!("access was {access}, but should only be r/w/a/read/write/admin");
							return None;
						},
					)))
				})
				.for_each(|ua| {
					access.insert(ua.clone());
					// Duplicating accesses
					if ua.access == Access::Write || ua.access == Access::Admin {
						access.insert((ua.user.clone(), Access::Read).into());
					}
					if ua.access == Access::Admin {
						access.insert((ua.user.clone(), Access::Write).into());
					}
				});
		}
	}
}

#[cfg(test)]
mod tests {
	use log::info;

	use crate::v1::db::{Access, Chunk};

	use super::DBChunk;

	#[test]
	fn test() {
		let mut chunk = DBChunk::from((None, "# Testing\n", "john"));
		// println!("{chunk:?}");
		// let ua = ;
		// println!("owner {}, user {}", &chunk.chunk.owner, &ua.0);
		assert!(chunk.has_access(&"john".into()));
		assert!(chunk.has_access(&"nina".into()) == false);
		let mut chunk = DBChunk::from((None, "# Testing\naccess:nina r", "john"));
		assert!(chunk.has_access(&"nina".into()) == true);
		// println!("{chunk:?}");
	}
}
