use lazy_static::lazy_static;
use proquint::Quintable;
use rand::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
	pub static ref REGEX_PROPERTY: Regex = Regex::new(format!("(?m){}", env!("REGEX_PROPERTY")).as_str()).unwrap();
	pub static ref REGEX_USERNAME: Regex = Regex::new(env!("REGEX_USERNAME")).unwrap();
	pub static ref REGEX_PASSWORD: Regex = Regex::new(env!("REGEX_PASSWORD")).unwrap();
}

lazy_static! {
	pub static ref DB_PATH: Option<String> = env::var("DB_PATH").ok();
	pub static ref DB_INIT: Option<String> = env::var("DB_INIT").ok();
	pub static ref DB_BACKUP_FOLDER: String = env::var("DB_BACKUP_FOLDER").unwrap_or("backups".into());
	pub static ref MEDIA_FOLDER: String = env::var("MEDIA_FOLDER").unwrap_or("media".into());
	pub static ref CACHE_PATH: String = env::var("CACHE_PATH").unwrap_or("cache.json".into());
	pub static ref WEB_DIST: String = env::var("WEB_DIST").unwrap_or("web".into());
	pub static ref PAGE_DIST: String = env::var("PAGE_DIST").unwrap_or("web".into());
	pub static ref BACKEND_DIST: String = env::var("BACKEND_DIST").unwrap_or("backend".into());
	pub static ref HOST: String =
		env::var("HOST").unwrap_or(format!("0.0.0.0:{}", env::var("PORT").unwrap_or("4000".into())));
}

pub const KEYWORD_BLACKLIST: [&str; 12] = [
	"admin", "root", "note", "chunk", "share", "access", "read", "write", "lock", "unlock", "public", "inherit",
];

/**
 * # Basic string normalizer
 * 1. Lowercases everything.
 * 1. Turns `[ -]` to spaces ` `.
 * 1. Only allows `[a-z0-9_]` through.
 */
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

/**
 * Describes a handled error.
 */
#[derive(Debug, PartialEq, Serialize, Eq)]
pub enum DbError {
	UserTaken,
	AuthError,
	InvalidUsername,
	InvalidPassword,
	InvalidChunk,
	NotFound,
}

use diff::Result::*;

pub fn diff_calc(left: &str, right: &str) -> Vec<String> {
	let diffs = diff::lines(left, right);
	// SO it'll be ["B44", ""]
	let out: Vec<String> = diffs.iter().fold(vec![], |mut acc, v| {
		match *v {
			Left(_l) => {
				if acc.last().and_then(|v| Some(v.starts_with("D"))) == Some(true) {
					// Add 1
					*acc.last_mut().unwrap() = format!("D{}", (&acc.last().unwrap()[1..].parse::<u32>().unwrap() + 1));
				} else {
					acc.push("D1".to_string());
				}
			}
			Both(_, _) => {
				if acc.last().and_then(|v| Some(v.starts_with("K"))) == Some(true) {
					// Add 1
					*acc.last_mut().unwrap() = format!("K{}", (&acc.last().unwrap()[1..].parse::<u32>().unwrap() + 1));
				} else {
					acc.push("K1".to_string());
				}
			}
			Right(l) => {
				acc.push(format!("A{}", l));
			}
		}
		acc
	});
	// info!("{out:?}");
	// println!("{diffs:?}");
	out
}

// /// Has to return something that's easy to merge into current data
// /// Easiest thing to merge would be index + data. Assuming iteration of front+back is same
// #[derive(Serialize, Default)]
// pub struct Page<T> {
// 	pub start_i: Option<usize>,
// 	pub start_id: Option<usize>,
// 	pub size: usize,
// 	pub data: Vec<T>,
// }

// #[derive(Deserialize, Default)]
// #[serde(default)]
// pub struct PageQuery {
// 	pub index: Option<usize>,
// 	pub page_size: Option<usize>,
// }
// impl PageQuery {
// 	pub fn is_empty(&self) -> bool {
// 		self.index.is_none() && self.page_size.is_none()
// 	}
// }


// pub fn maybe_paginate<T, E: Serialize, I: IntoIterator<Item = T>, F: Fn(T) -> E>(
// 	(query, iter, map): (&PageQuery, I, &F),
// ) -> Value {
// 	let iter = iter.into_iter();
// 	if !query.is_empty() {
// 		let page = query.index.unwrap();
// 		let page_size = query.page_size.unwrap();
// 		let items = iter
// 			.skip(page * page_size)
// 			.take(query.page_size.unwrap())
// 			.map(map)
// 			.collect();
// 		let page = Page {
// 			start_at: page,
// 			size: page_size,
// 			data: items,
// 		};
// 		json!(page)
// 	} else {
// 		let items = iter.map(map).collect::<Vec<_>>();
// 		json!(items)
// 	}
// }
