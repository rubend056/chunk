use crate::{
	utils::{get_secs, DbError, SECS_IN_DAY},
	v1::*,
};
use axum::{extract::Extension, http::header, response::IntoResponse, Json};
use core::convert::TryFrom;
use hyper::{Method, StatusCode};
use lazy_static::lazy_static;
use pasetors::claims::{Claims, ClaimsValidationRules};
use pasetors::keys::{AsymmetricKeyPair, Generate};
use pasetors::token::{TrustedToken, UntrustedToken};
use pasetors::{public, version4::V4, Public};
use serde::Serialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{
	ends::DB,
	socket::{ResourceMessage, ResourceSender},
};

lazy_static! {
	static ref KP: AsymmetricKeyPair::<V4> = AsymmetricKeyPair::<V4>::generate().unwrap();
}

pub async fn login(
	Extension(db): Extension<DB>,
	Json((user, pass)): Json<(String, String)>,
) -> Result<impl IntoResponse, DbError> {
	let db = db.write().unwrap();
	db.auth
		.login(&user, &pass)
		.and_then(|_| {
			// Create token

			let mut claims = Claims::new().unwrap();

			claims.issuer("chunk.anty.dev").unwrap();
			claims.add_additional("user", user.clone()).unwrap();

			let iat = OffsetDateTime::from_unix_timestamp(get_secs().try_into().unwrap())
				.unwrap()
				.format(&Rfc3339)
				.unwrap();
			let exp = get_secs() + SECS_IN_DAY * 7; // 7 days
			let exp = OffsetDateTime::from_unix_timestamp(exp.try_into().unwrap())
				.unwrap()
				.format(&Rfc3339)
				.unwrap();

			claims.issued_at(&iat).unwrap();
			claims.expiration(&exp).unwrap();

			// Generate the keys and sign the claims.
			let pub_token = public::sign(&KP.secret, &claims, None, None).unwrap();

			Ok([(
				header::SET_COOKIE,
				format!(
					"auth={pub_token}; SameSite=Strict; Max-Age={}; Path=/; Secure;",
					60/*sec*/*60/*min*/*24/*hr*/*7 /*days*/ /*= a week in seconds*/
				),
			)])
		})
		.or_else(|err| {
			error!("Failed login for '{}' with pass '{}': {:?}.", &user, &pass, &err);
			Err(err)
		})
}
pub async fn register(
	Extension(db): Extension<DB>,
	Json((user, pass)): Json<(String, String)>,
) -> Result<impl IntoResponse, DbError> {
	let mut db = db.write().unwrap();

	db.auth
		.new_user(&user, &pass)
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
	Extension(db): Extension<DB>,
	Json((user, old_pass, pass)): Json<(String, String, String)>,
) -> Result<impl IntoResponse, DbError> {
	let mut db = db.write().unwrap();

	db.auth
		.reset(&user, &pass, &old_pass)
		.and_then(|_| {
			info!("User password reset '{user}'.");
			Ok("User pass reset.")
		})
		.or_else(|err| {
			error!("Failed password reset for '{user}' with old_pass '{old_pass}': {err:?}.");
			Err(err)
		})
}
pub async fn logout_all(
	Extension(db): Extension<DB>,
	Extension(user_claims): Extension<UserClaims>,
	Extension(tx_r): Extension<ResourceSender>,
) -> Result<impl IntoResponse, DbError> {
	db.write()
		.unwrap()
		.auth
		.users
		.get(&user_claims.user)
		.unwrap()
		.write()
		.unwrap()
		.reset_not_before();

	tx_r
		.send(ResourceMessage {
			close_for_users: Some([user_claims.user.clone()].into()),
			..Default::default()
		})
		.unwrap();

	Ok(())
}

use axum::{http::Request, middleware::Next, response::Response};

pub async fn auth_required<B>(req: Request<B>, next: Next<B>) -> Result<Response, impl IntoResponse> {
	if req.extensions().get::<TrustedToken>().is_none() {
		Err(DbError::AuthError)
	} else {
		Ok(next.run(req).await)
	}
}

pub async fn public_only_get<B>(req: Request<B>, next: Next<B>) -> Result<Response, impl IntoResponse> {
	if req.extensions().get::<TrustedToken>().is_none() && *req.method() != Method::GET {
		Err(DbError::AuthError)
	} else {
		Ok(next.run(req).await)
	}
}

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
		if let Some(auth_value) = auth_header.iter().find(|(k, _v)| *k == "auth").and_then(|v| Some(v.1)) {
			if let Some((token, _user_claims)) = get_valid_token(auth_value) {
				let claims = token.payload_claims().unwrap();
				let user_claim = claims.get_claim("user").unwrap().as_str().unwrap();
				let iat_claim = claims.get_claim("iat").unwrap().as_str().unwrap();
				let iat_unix = OffsetDateTime::parse(iat_claim, &Rfc3339).unwrap().unix_timestamp() as u64;
				let mut iat_good = false;

				let db = req.extensions().get::<DB>().unwrap();
				if let Ok(user) = db.read().unwrap().auth.get_user(user_claim) {
					iat_good = user.verify_not_before(iat_unix);
				}

				if iat_good {
					user_claims = _user_claims;
					req.extensions_mut().insert(token);
				}
			}
		}
	}

	req.extensions_mut().insert(user_claims);

	Ok(next.run(req).await)
}

pub async fn user(Extension(_db): Extension<DB>, Extension(user_claims): Extension<UserClaims>) -> impl IntoResponse {
	Json(user_claims)
}

#[derive(Clone, Serialize)]
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

	if let Ok(untrusted_token) = UntrustedToken::<Public, V4>::try_from(token) {
		if let Ok(trusted_token) = public::verify(&KP.public, &untrusted_token, &validation_rules, None, None) {
			let claims = trusted_token.payload_claims().unwrap().clone();
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
