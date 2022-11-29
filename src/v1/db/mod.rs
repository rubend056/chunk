use serde::{Deserialize, Serialize};
/** Designing a new Data Structure that would allow for all queries/insertions/serializations to efficiently happen */

/**
 - Different visualization options
 - Editing
	 - ![](web/src/assets/icons/card-text.svg) **Shank/Edit** -> selected chunk + children up to 4N (1N default) an editor
 - Viewing
	 - ![](web/src/assets/icons/clipboard.svg) **Notes** -> chunks ordered by recent side by side
	 - ![](web/src/assets/icons/grid.svg) **Well** -> selected chunk children on a grid
	 - ![](web/src/assets/icons/diagram-2-fill.svg) **Graph** -> nodes in a tree
				 (S)	-> (R r)
				(S) -> ()

				(R) -> (S w) ->
								\> (S r)
		Querying this with different views

*/
use std::collections::{HashMap, HashSet};

use crate::v1::{
	chunk::{Chunk, ChunkMeta},
	user::User,
};

use super::chunk::Access;

#[derive(Serialize, Deserialize)]
pub struct DBData {
	pub chunks: Vec<Chunk>,
	pub users: Vec<User>,
}


#[derive(Serialize)]
pub enum ChunkType {
	Owner,
	Access(Access),
}
pub type ChunkView = (Chunk, ChunkType);
#[derive(Serialize)]
pub struct ChunkTree(pub Chunk, pub Option<Vec<ChunkTree>>);

type ChunkAndMeta = (Chunk, ChunkMeta);
type UsersToNotify = HashSet<String>;

#[derive(Default, Debug, Serialize)]
pub struct DB {
	chunks: HashMap<String, ChunkAndMeta>,
	users: HashMap<String, User>,

	// For faster ref->id lookups, that means we need to update this on create/remove/modify
	ref_id: HashMap<String, Vec<String>>,
}

mod db;
mod db_data;

#[cfg(test)]
mod tests;
