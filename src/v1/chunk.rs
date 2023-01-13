use std::{
	collections::{HashMap, HashSet},
	fmt::Debug,
};

use log::error;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::{gen_proquint, get_secs, standardize, DbError, REGEX_ACCESS, REGEX_TITLE, REGEX_USERNAME};



// pub type UserAccess = (String, Access);


//---------------------------------- META --------------------

pub type UserRef = (Option<String>, String);

pub enum CacheValue {
	String(String),
	Int(i32),
}
pub enum CacheDirection {
	Up,
	Down,
}
/**
 * Calculates current value from Parents/Children
 */
type CacheFn = fn(&mut ChunkAndMeta, &Vec<&ChunkAndMeta>) -> String;
#[derive(Clone)]
pub struct GraphCache {
	key: String,
	value: String,
	valid: bool,
	/**
	 *  If true means second argument is parents and will, false -> children
	 */
	up: bool,
	function: CacheFn,
}
impl Debug for GraphCache {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("{}:{} valid:{}", &self.key, &self.value, &self.valid))
	}
}
// impl From<(&str, &'static CacheFn)> for GraphCache {
// 	fn from((key, function): (&str,&'static CacheFn)) -> Self {
// 		GraphCache {
// 			key: key.into(),
// 			function,
// 			valid: false,
// 			value: "".into(),
// 		}
// 	}
// }

#[derive(Debug, Clone, Serialize, Default)]
pub struct ChunkMeta {
	pub _ref: String, // Standardized title
	pub title: String,
	pub _refs: HashSet<UserRef>, // Standardized references to other chunks
	pub access: HashSet<UserAccess>,
	#[serde(skip)]
	pub fields: HashMap<String, GraphCache>,
}

impl PartialEq for ChunkMeta {
	fn eq(&self, other: &Self) -> bool {
		self._ref == other._ref && self.title == other.title && self._refs == other._refs && self.access == other.access
	}
	fn ne(&self, other: &Self) -> bool {
		!self.eq(other)
	}
}

impl ChunkMeta {
	pub fn to_string(&self, value: &String) -> String {
		let mut value = value.clone();
		if !self._ref.is_empty() {
			value = REGEX_TITLE
				.replace(
					&value,
					format!(
						"# {}{}",
						self.title,
						if self._refs.len() > 0 {
							format!(
								" -> {}",
								self
									._refs
									.iter()
									.map(|(u, r)| format!("{}{}", u.clone().unwrap_or("".into()), r))
									.reduce(|a, b| format!("{}, {}", a, b))
									.unwrap()
							)
						} else {
							"".into()
						}
					),
				)
				.to_string();
		}

		// Remove all replacements
		value = REGEX_ACCESS.replace_all(&value, "").to_string();
		if let Some(v) = self
			.access
			.iter()
			.map(|(u, a)| {
				format!(
					"{} {}",
					u.clone(),
					match a {
						Access::Read => "R",
						Access::Write => "W",
						Access::Admin => "A",
					}
				)
			})
			.reduce(|a, b| format!("{}, {}", a, b))
		{
			value = format!("{value}\n{}", format!("Access: {}", v));
		}

		value
	}
}

fn child_count(v: &mut ChunkAndMeta, o: &Vec<&ChunkAndMeta>) -> String {
	o.len().to_string()
}
// const cache_default: [(String, GraphCache);1] = ;



impl From<&String> for ChunkMeta {
	// let j = Fn()
	// Extracts metadata from Chunk
	fn from(value: &String) -> Self {
		let mut _ref = "".into();
		let mut title = "".into();
		let mut _refs = HashSet::<UserRef>::default();
		let mut access = HashSet::<UserAccess>::default();
		let mut cache = HashMap::<String, GraphCache>::from([(
			"child_count".into(),
			GraphCache {
				key: "child_count".into(),
				value: String::from(""),
				function: child_count,
				up: false,
				valid: false,
			},
		)]);

		{
			// Extracting  # title/ref -> ref,ref,ref
			if let Some(captures) = REGEX_TITLE.captures(&value) {
				if let Some(m) = captures.get(1) {
					_ref = standardize(m.as_str());
					title = m.as_str().into();
				}
				if let Some(m) = captures.get(2) {
					_refs = m
						.as_str()
						.split(",")
						.map(|v| {
							let vs = v.split(":").collect::<Vec<_>>();
							if vs.len() == 2 {
								(Some(standardize(vs[0])), standardize(vs[1]))
							} else if vs.len() == 1 {
								(None, standardize(vs[0]))
							} else {
								panic!("Has to be something");
							}
						})
						.collect();
				}
			}
		}

		extract_access(value, &mut access);

		ChunkMeta {
			_ref,
			title,
			_refs,
			access,
			fields: cache,
		}
	}
}
impl From<&Chunk> for ChunkMeta {
	fn from(chunk: &Chunk) -> Self {
		let mut meta = ChunkMeta::from(&chunk.value);
		meta
	}
}

#[cfg(test)]
mod tests {
	use crate::utils::REGEX_TITLE;

	#[test]
	fn title_regex() {
		assert!(REGEX_TITLE.is_match("# Groceries\n"));
		assert!(REGEX_TITLE.is_match("# Work -> pamup_fupin\n"));
		assert!(REGEX_TITLE.is_match("# Work -> pamup_fupin, lopis_muzuz\n"));
		assert!(REGEX_TITLE.is_match("# Test ->  pamup_fupin, pamit_losab_torak\n"));
		
	}
}
