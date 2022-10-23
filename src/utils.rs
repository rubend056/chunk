use lazy_static::lazy_static;
use proquint::Quintable;
use rand::prelude::*;
use regex::Regex;
use std::{
	env,
	time::{SystemTime, UNIX_EPOCH},
};

pub fn get_secs() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("Before UNIX_EPOCH")
		.as_secs()
}

pub fn gen_proquint() -> String {
	random::<u32>().to_quint()
}

lazy_static! {
	pub static ref REGEX_TITLE: Regex = Regex::new(env!("REGEX_TITLE")).unwrap();
	pub static ref REGEX_ACCESS: Regex = Regex::new(env!("REGEX_ACCESS")).unwrap();
	pub static ref REGEX_USER: Regex = Regex::new(env!("REGEX_USER")).unwrap();
}

#[derive(Debug, Default)]
pub struct Env {
	pub db_path: Option<String>,
	pub db_init: Option<String>,
	pub web_dist: String,
	pub host: String,
}
lazy_static! {
	pub static ref DB_PATH: Option<String> = env::var("DB_PATH").ok();
	pub static ref DB_INIT: Option<String> = env::var("DB_INIT").ok();
	pub static ref DB_BACK_FOLDER: String = env::var("DB_BACK_FOLDER").unwrap_or("backups".to_string());
	pub static ref CACHE_PATH: String = env::var("CACHE_PATH").unwrap_or("cache.json".to_string());
	pub static ref WEB_DIST: String = env::var("WEB_DIST").unwrap_or("web".to_string());
	pub static ref HOST: String = env::var("HOST").unwrap_or(format!(
		"0.0.0.0:{}",
		env::var("PORT").unwrap_or("4000".to_string())
	));
}

// use std::collections::hash_map::DefaultHasher;
// use std::hash::{Hash};
// lazy_static!{
// 	pub static ref HASHER:DefaultHasher = DefaultHasher::new();
// }
