// use serde::Deserialize;
use std::env;

#[derive(Debug)]
pub struct Env {
    pub db_path: String,
    pub db_init: Option<String>,
    pub web_dist: String,
    pub host: String,
}

// pub static ENV_VARS: Env = get_vars();

pub fn get_vars() -> Env {
    Env {
        db_path: env::var("DB_PATH").unwrap_or("db.json".to_string()),
        db_init: env::var("DB_INIT").ok(),
        web_dist: env::var("WEB_DIST").unwrap_or("web".to_string()),
        host: env::var("HOST").unwrap_or(format!(
            "0.0.0.0:{}",
            env::var("PORT").unwrap_or("4000".to_string())
        )),
    }
}
