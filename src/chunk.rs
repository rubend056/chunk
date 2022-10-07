use serde::{Deserialize, Serialize};
use std::{
    char::ToLowercase,
    time::{SystemTime, UNIX_EPOCH},
};

/**
 * Allows for a unix timestamp (seconds since epoch) until forever
 */
type UTC = u64;

/*
* Can hopefully model a chunk of information in your brain
*/
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Chunk {
    /**
     * `id = title` (Lowercased, trimmed, replacing space by underscore and removing all [^a-z_0-9]). This allows pretty formatting of titles but standardizes the ids.
     */
    pub _id: String,
    pub value: String,
    pub created: UTC,
    pub modified: UTC,
}
impl Chunk {
    // pub fn id (&self) -> String {self._id}
    pub fn new(value: String) -> Chunk {
        let epoch_seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs(),
            Err(_) => panic!("Can't get unix epoch?"),
        };

        let _id = value
            .trim()
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
            .collect();

        Chunk {
            _id,
            value,
            created: epoch_seconds,
            modified: epoch_seconds,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    user: String,
    pass: String,
    salt: String, // (for brute force attacks)
}
