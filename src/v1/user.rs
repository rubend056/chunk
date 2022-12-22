use argon2::{
	password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
	Argon2,
};
use serde::{Deserialize, Serialize};

use crate::utils::{get_secs, DbError, REGEX_USERNAME, REGEX_PASSWORD};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
	pub user: String,
	pass: String, // PHC String
	#[serde(default = "get_secs")]
	pub not_before: u64,
}

impl User {
	fn _verify(&self, pass: &str) -> bool {
		// PHC string -> PasswordHash.
		let parsed_hash = PasswordHash::new(&self.pass).expect("Error parsing existing password field");

		// Compare pass hash vs PasswordHash
		if let Err(_) = Argon2::default().verify_password(pass.as_bytes(), &parsed_hash) {
			return false;
		};
		true
	}
	fn hash(pass: &str) -> Result<String, DbError> {
		if !REGEX_PASSWORD.is_match(pass) {
			return Err(DbError::InvalidPassword);
		}

		let salt = SaltString::generate(&mut OsRng);
		Ok(Argon2::default().hash_password(pass.as_bytes(), &salt).unwrap().to_string())
	}
	pub fn new(user: &str, pass: &str) -> Result<Self, DbError> {
		if !REGEX_USERNAME.is_match(user) {
			return Err(DbError::InvalidUsername);
		}

		Ok(User {
			user: user.into(),
			pass: User::hash(&pass)?,
			not_before: get_secs(),
		})
	}

	pub fn verify(&self, pass: &str) -> bool {
		if get_secs() < self.not_before {
			return false;
		}
		self._verify(pass)
	}

	pub fn reset_pass(&mut self, old_pass: &str, pass: &str) -> Result<(), DbError> {
		if !self._verify(old_pass) {
			return Err(DbError::AuthError);
		}
		self.pass = User::hash(pass)?;
		self.not_before = get_secs();

		Ok(())
	}
}
