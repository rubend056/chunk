
use serde::{Deserialize, Serialize};
use crate::utils::{get_secs};

mod src;
mod blacklist;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
	pub user: String,
	pass: String, // PHC String
	#[serde(default = "get_secs")]
	pub not_before: u64,
}