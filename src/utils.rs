use lazy_static::lazy_static;
use proquint::Quintable;
use rand::prelude::*;
use regex::Regex;
use serde::Serialize;
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
pub const SECS_IN_HOUR: u64 = 60 * 60;
pub const SECS_IN_DAY: u64 = SECS_IN_HOUR * 24;

pub fn gen_proquint() -> String {
	random::<u32>().to_quint()
}

lazy_static! {
	pub static ref REGEX_TITLE: Regex = Regex::new(env!("REGEX_TITLE")).unwrap();
	pub static ref REGEX_ACCESS: Regex = Regex::new(format!("(?im){}", env!("REGEX_ACCESS")).as_str()).unwrap();
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
	pub static ref DB_BACKUP_FOLDER: String = env::var("DB_BACKUP_FOLDER").unwrap_or("backups".into());
	pub static ref MEDIA_FOLDER: String = env::var("MEDIA_FOLDER").unwrap_or("media".into());
	pub static ref CACHE_PATH: String = env::var("CACHE_PATH").unwrap_or("cache.json".into());
	pub static ref WEB_DIST: String = env::var("WEB_DIST").unwrap_or("web".into());
	pub static ref HOST: String =
		env::var("HOST").unwrap_or(format!("0.0.0.0:{}", env::var("PORT").unwrap_or("4000".into())));
}

#[derive(Debug, PartialEq, Serialize, Eq)]
pub enum DbError {
	UserTaken,
	AuthError,
	InvalidUser,
	// InvalidChunk,
	NotFound,
}
