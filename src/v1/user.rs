// use core::num::dec2flt::parse;

use argon2::{
	password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
	Argon2,
};
use serde::{Deserialize, Serialize};

use crate::utils::{DbError, REGEX_USERNAME};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
	pub user: String,
	pass: String, // PHC String
}

impl User {
	pub fn new(user: String, pass: String) -> Result<Self, DbError> {
		if !REGEX_USERNAME.is_match(user.as_str()) {
			return Err(DbError::InvalidUsername);
		}

		let salt = SaltString::generate(&mut OsRng);

		Ok(User {
			user,
			pass: Argon2::default()
				.hash_password(pass.as_ref(), &salt)
				.unwrap()
				.to_string(),
		})
	}
	/**
	 * Used when loggin in and verifying a password
	 */
	pub fn verify(&self, pass: &str) -> bool {
		// Verify password against PHC string.
		let parsed_hash = PasswordHash::new(&self.pass).expect("Error parsing existing password field");
		if let Err(_) = Argon2::default().verify_password(pass.as_ref(), &parsed_hash) {
			return false;
		};
		true
	}
	/**
	 * Used when creating, or resetting user password
	 */
	pub fn reset_pass(&mut self, old_pass: &str, pass: &str) -> Result<(), DbError> {
		// Verify password against PHC string.
		let parsed_hash = PasswordHash::new(&self.pass).expect("Error parsing existing password field");
		let salt = parsed_hash.salt.expect("Salt must exist");

		// Argon2 with default params (Argon2id v19)
		let argon2 = Argon2::default();

		// Hash password to PHC string ($argon2id$v=19$...)
		if let Err(_) = Argon2::default().verify_password(old_pass.as_ref(), &parsed_hash) {
			return Err(DbError::AuthError);
		};

		self.pass = argon2
			.hash_password(pass.as_ref(), &salt)
			.expect("Hashing shouldn't fail")
			.to_string();

		Ok(())
	}
}
