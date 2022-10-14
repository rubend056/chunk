use lazy_regex::regex;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/**
 * Allows for a unix timestamp (seconds since epoch) until forever
 */
pub type UTC = u128;

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
    pub fn new(value: &String) -> Result<Chunk, &'static str> {
        let epoch_millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_millis(),
            Err(_) => return Err("Can't get unix epoch?"),
        };

        let title_rx =
            regex!(r"^#  *(?P<title>(?: *[\w]+)+) *(?:[-=]> *(?P<relations>(?:,? *[\w]+)+) *)?$"m);
        // let end_space_rx = regex!("[ \t]+"m);

        // For now we'll trim anything before the first # which we'll assume is the title
        if let Some(captures) = title_rx.captures(value.as_str()) {
            if let (Some(m0), Some(m1)) = (captures.get(0), captures.get(1)) {
                let mut value = value.clone();
                value.replace_range(..m0.start(), "");

                let _id = m1
                    .as_str()
                    .trim()
                    .to_string()
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

                return Ok(Chunk {
                    _id,
                    value,
                    created: epoch_millis,
                    modified: epoch_millis,
                });
            }
        }

        Err("No title on chunk")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    user: String,
    pass: String,
    salt: String, // (for brute force attacks)
}



