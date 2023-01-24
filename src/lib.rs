use std::collections::HashMap;

use serde::{Serialize, Deserialize};


pub mod utils;
pub mod v1;
pub mod cache;
pub mod backup;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MediaEntry {
	Ref(String), // Means entry hash maps to another hash, meaning conversion yielded a different hash
	Entry {
		user: String,
		#[serde(with = "v1::ends::MatcherType", rename = "type")]
		_type: infer::MatcherType,
	},
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Cache {
	pub last_backup: u64,
	pub media: HashMap<String, MediaEntry>,
}