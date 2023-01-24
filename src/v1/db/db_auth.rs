use std::sync::{Arc, RwLock, Weak};

use crate::{
	utils::{DbError, KEYWORD_BLACKLIST},
	v1::user::User,
};

use super::DBMap;

#[derive(Default)]
pub struct DBAuth {
	pub users: DBMap<String, Arc<RwLock<User>>>,

	/// Groups/roles
	pub groups: DBMap<String, Vec<Weak<RwLock<User>>>>,
}

impl DBAuth {
	pub fn new_user(&mut self, user: &str, pass: &str) -> Result<(), DbError> {
		if self.users.get(user).is_some() || self.groups.get(user).is_some() {
			return Err(DbError::UserTaken);
		}
		if KEYWORD_BLACKLIST.iter().any(|ub| user.contains(ub)) {
			return Err(DbError::InvalidUsername);
		}

		let user_instance = User::new(user, pass)?;

		self.users.insert(user.into(), Arc::new(RwLock::new(user_instance)));

		Ok(())
	}
	pub fn get_user(&self, user: &str) -> Result<User, DbError> {
		self
			.users
			.get(user)
			.map(|u| u.read().unwrap().to_owned())
			.ok_or(DbError::NotFound)
	}
	pub fn login(&self, user: &str, pass: &str) -> Result<(), DbError> {
		let user = self.users.get(user).ok_or(DbError::AuthError)?.read().unwrap();
		if !user.verify(pass) {
			return Err(DbError::AuthError);
		}
		Ok(())
	}
	pub fn reset(&mut self, user: &str, pass: &str, old_pass: &str) -> Result<(), DbError> {
		let mut user = self.users.get(user).ok_or(DbError::AuthError)?.write().unwrap();

		user.reset_pass(old_pass, pass)
	}
}

#[cfg(test)]
mod tests {
	use rand::distributions::{Alphanumeric, DistString};

	use super::*;

	#[test]
	fn users() {
		let mut db = DBAuth::default();
		assert_eq!(
			db.new_user("Nana3", "1234"),
			Err(DbError::InvalidUsername),
			"Username characters invalid, only lowercase"
		);
		assert_eq!(
			db.new_user("Nana&", "1234"),
			Err(DbError::InvalidUsername),
			"Username characters invalid, no special"
		);
		assert_eq!(
			db.new_user(":nana", "1234"),
			Err(DbError::InvalidUsername),
			"Username characters invalid, no special"
		);
		assert_eq!(
			db.new_user("na", "1234"),
			Err(DbError::InvalidUsername),
			"Username >= 3 in size"
		);
		assert_eq!(
			db.new_user("nan", "12"),
			Err(DbError::InvalidPassword),
			"Password >= 6 in size"
		);
		assert_eq!(
			db.new_user("nan", &Alphanumeric.sample_string(&mut rand::thread_rng(), 70)),
			Err(DbError::InvalidPassword),
			"Password <= 64 in size"
		);
		assert!(db.new_user("nina", "nina's pass").is_ok());

		// assert_eq!(db.users.len(), 1);

		assert!(db.login("nina", "wrong_pass").is_err(), "Password is wrong");
		assert!(db.login("nana", "wrong_pass").is_err(), "User nana doesn't exist");
		assert!(db.login("nina", "nina's pass").is_ok(), "Login success");
	}
}
