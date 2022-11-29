use super::{DBData, DB};

impl From<DB> for DBData {
	fn from(value: DB) -> Self {
		DBData {
			chunks: value.chunks.into_iter().map(|c| c.1 .0).collect(),
			users: value.users.into_iter().map(|c| c.1).collect(),
		}
	}
}
impl DBData {
	pub fn new(value: &DB) -> Self {
		DBData {
			chunks: value.chunks.iter().map(|c| c.1 .0.clone()).collect(),
			users: value.users.iter().map(|c| c.1.clone()).collect(),
		}
	}
}
