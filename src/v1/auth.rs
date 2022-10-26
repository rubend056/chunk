use crate::{utils::DbError, v1::*};
use axum::{
	extract::{ws::WebSocket, Extension, Path, WebSocketUpgrade},
	http::header,
	response::{ErrorResponse, IntoResponse},
	Json,
};
use core::convert::TryFrom;
use hyper::StatusCode;
use lazy_static::lazy_static;
use pasetors::claims::{Claims, ClaimsValidationRules};
use pasetors::keys::{AsymmetricKeyPair, Generate};
use pasetors::token::{TrustedToken, UntrustedToken};
use pasetors::{public, version4::V4, Public};
use tower_http::set_header::SetResponseHeader;

use super::ends::DB;

lazy_static! {
	static ref KP: AsymmetricKeyPair::<V4> = AsymmetricKeyPair::<V4>::generate().unwrap();
}

pub async fn login(
	Json((user, pass)): Json<(String, String)>,
	Extension(db): Extension<DB>,
) -> Result<impl IntoResponse, DbError> {
	let db = db.write().unwrap();
	db.login(&user, &pass)
		.and_then(|_| {
			// Create token
			// Setup the default claims, which include `iat` and `nbf` as the current time and `exp` of one hour.
			// Add a custom `data` claim as well.
			let mut claims = Claims::new().unwrap();
			claims.issuer("chunk.anty.dev").unwrap();
			claims.add_additional("user", user.clone()).unwrap();
			claims.non_expiring();

			// Generate the keys and sign the claims.

			let pub_token = public::sign(&KP.secret, &claims, None, Some(b"implicit assertion")).unwrap();

			Ok([(header::SET_COOKIE, format!("auth={pub_token}; SameSite=Strict"))])
		})
		.or_else(|err| {
			error!("Failed login for '{}' with pass '{}': {:?}.", &user, &pass, &err);
			Err(err)
		})
}
pub async fn register(
	Json((user, pass)): Json<(String, String)>,
	Extension(db): Extension<DB>,
) -> Result<impl IntoResponse, DbError> {
	let mut db = db.write().unwrap();

	db.new_user(user.clone(), pass.clone())
		.and_then(|_| {
			info!("User created '{}'.", &user);
			Ok("User created.")
		})
		.or_else(|err| {
			error!("Failed register for '{}' with pass '{}': {:?}.", &user, &pass, &err);
			Err(err)
		})
}
pub async fn reset(
	Json((user,old_pass, pass)): Json<(String, String, String)>,
	Extension(db): Extension<DB>,
) -> Result<impl IntoResponse, DbError> {
	let mut db = db.write().unwrap();

	db.reset(&user, &pass, &old_pass)
		.and_then(|_| {
			info!("User password reset '{}'.", &user);
			Ok("User pass reset.")
		})
		.or_else(|err| {
			error!(
				"Failed password reset for '{}' with old_pass '{}': {:?}.",
				&user, &old_pass, &err
			);
			Err(err)
		})
}

use axum::{http::Request, middleware::Next, response::Response};

pub async fn authenticate<B>(mut req: Request<B>, next: Next<B>) -> Result<Response, StatusCode> {
	let mut user_claims = UserClaims::default();

	if let Some(auth_header) = req
		.headers()
		.get(header::COOKIE)
		.and_then(|header| {
			// info!("Header tostr {:?}", header.to_str().ok());
			header.to_str().ok()
		})
		.and_then(|v| {
			Some(v.split(";").fold(vec![], |mut acc, v| {
				let kv = v.split("=").collect::<Vec<_>>();
				if kv.len() == 2 {
					acc.push((kv[0].trim(), kv[1]))
				}
				acc
			}))
		}) {
		if let Some(auth_value) = auth_header.iter().find(|(k, v)| *k == "auth").and_then(|v| Some(v.1)) {
			if let Some((token, _user_claims)) = get_valid_token(auth_value) {
				user_claims = _user_claims;
				req.extensions_mut().insert(token);
			}
		}
	}

	req.extensions_mut().insert(user_claims);

	return Ok(next.run(req).await);
}

#[derive(Clone)]
pub struct UserClaims {
	pub user: String,
}
impl Default for UserClaims {
	fn default() -> Self {
		Self { user: "public".into() }
	}
}
impl From<&Claims> for UserClaims {
	fn from(claims: &Claims) -> Self {
		Self {
			user: claims
				.get_claim("user")
				.and_then(|v| v.as_str())
				.unwrap_or("public")
				.into(),
		}
	}
}

fn get_valid_token(token: &str) -> Option<(TrustedToken, UserClaims)> {
	let mut validation_rules = ClaimsValidationRules::new();
	validation_rules.validate_issuer_with("chunk.anty.dev");
	validation_rules.allow_non_expiring();

	if let Ok(untrusted_token) = UntrustedToken::<Public, V4>::try_from(token) {
		if let Ok(trusted_token) = public::verify(
			&KP.public,
			&untrusted_token,
			&validation_rules,
			None,
			Some(b"implicit assertion"),
		) {
			let claims = trusted_token.payload_claims().unwrap().clone();
			println!("{:?}", claims.get_claim("data"));
			println!("{:?}", claims.get_claim("iat"));

			return Some((trusted_token, UserClaims::from(&claims)));
		}
	}


	None
}

#[cfg(test)]
mod tests {
	#[test]
	fn token() {}
}
