use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::DbError;

/**
 * Allows for a unix timestamp (seconds since epoch) until forever
 */
pub type UTC = u128;

/*
* Can hopefully model a chunk of information in your brain
*/
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Chunk {
	/**
	 * `id = title` (Lowercased, trimmed, replacing space by underscore and removing all [^a-z_0-9]). This allows pretty formatting of titles but standardizes the ids.
	 */
	pub _id: String,
	pub value: String,
	pub created: UTC,
	pub modified: UTC,
}
impl Chunk {
	// pub fn id (&self) -> String {self._id}
	pub fn new(value: &String) -> Result<Chunk, DbError> {
		let epoch_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
			Ok(n) => n.as_micros(),
			Err(_) => panic!("Before UNIX_EPOCH"),
		};

		lazy_static! {
			pub static ref REGEX_TITLE: Regex = Regex::new(env!("REGEX_TITLE")).unwrap();
		}

		// For now we'll trim anything before the first # which we'll assume is the title
		if let Some(captures) = REGEX_TITLE.captures(value.as_str()) {
			if let (Some(m0), Some(m1)) = (captures.get(0), captures.get(1)) {
				let mut value = value.clone();
				value.replace_range(..m0.start(), "");

				let _id = m1
					.as_str()
					.trim()
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
					.collect();

				return Ok(Chunk {
					_id,
					value,
					created: epoch_millis,
					modified: epoch_millis,
				});
			}
		}

		Err(DbError::InvalidChunk)
	}
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
	user: String,
	pass: String,
	salt: String, // (for brute force attacks)
}
