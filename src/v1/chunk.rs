use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::utils::{get_secs, DbError, REGEX_ACCESS, REGEX_TITLE, REGEX_USER};

#[derive(Serialize, Hash, Eq, PartialEq, Clone, Debug)]
pub enum Access {
	Read,
	Write,
}

pub type UserAccess = (String, Access);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Chunk {
	pub id: String,
	pub value: String,
	pub owner: String,
	pub created: u64,
	pub modified: u64,
}
impl Chunk {
	pub fn new(id: String, value: String, owner: String) -> Result<Self, DbError> {
		if !REGEX_USER.is_match(owner.as_str()) {
			return Err(DbError::InvalidUser);
		}

		let secs = get_secs();
		let chunk = Chunk {
			id,
			value,
			owner,
			created: secs,
			modified: secs,
		};

		Ok(chunk)
	}
}

//---------------------------------- META --------------------

pub fn standardize(v: &str) -> String {
	v.trim()
		.to_lowercase()
		.chars()
		.map(|v| match v {
			'-' => '_',
			' ' => '_',
			_ => v,
		})
		.filter(|v| match v {
			'a'..='z' => true,
			'0'..='9' => true,
			'_' => true,
			_ => false,
		})
		.collect()
}
pub fn standardize_pretty(v: &str) -> String {
	v.trim()
		.chars()
		.map(|v| match v {
			'-' => ' ',
			'_' => ' ',
			_ => v,
		})
		.filter(|v| match v {
			'A'..='Z' => true,
			'a'..='z' => true,
			'0'..='9' => true,
			' ' => true,
			_ => false,
		})
		.collect()
}

pub type UserRef = (Option<String>, String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChunkMeta {
	pub _ref: String, // Standardized title
	pub title: String,
	pub _refs: HashSet<UserRef>, // Standardized references to other chunks
	pub access: HashSet<UserAccess>,
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

impl From<&String> for ChunkMeta {
	// Extracts metadata from Chunk
	fn from(value: &String) -> Self {
		let mut _ref = "".into();
		let mut title = "".into();
		let mut _refs = HashSet::<UserRef>::default();
		let mut access = HashSet::<UserAccess>::default();

		{
			// Extracting  # title/ref -> ref,ref,ref
			if let Some(captures) = REGEX_TITLE.captures(&value) {
				if let Some(m) = captures.get(1) {
					_ref = standardize(m.as_str());
					title = standardize_pretty(m.as_str());
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

		{
			// Extracting  access/share
			for capture in REGEX_ACCESS.captures_iter(&value) {
				if let Some(m) = capture.get(1) {
					m.as_str()
						.to_lowercase()
						.split(",")
						.map(|ua| {
							let user_access = ua.trim().split(" ").collect::<Vec<_>>();
							if user_access.len() < 2 {
								panic!("user_access is NEVER less than 2 in length");
							}
							let (user, access) = (user_access[0], user_access[1]);

							if !REGEX_USER.is_match(user_access[0]) {
								panic!("user doesn't match user regex");
							}

							(
								user.into(),
								if access == "r" || access == "read" {
									Access::Read
								} else if access == "w" || access == "write" {
									Access::Write
								} else {
									panic!("access should be r/w/read/write ONLY");
								},
							)
						})
						.for_each(|ua| {
							access.insert(ua.clone());
							// Duplicating read access for write access users
							if ua.1 == Access::Write {
								access.insert((ua.0, Access::Read));
							}
						});
				}
			}
		}

		ChunkMeta {
			_ref,
			title,
			_refs,
			access,
		}
	}
}
