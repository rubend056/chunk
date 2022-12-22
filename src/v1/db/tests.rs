

use crate::v1::db::db_chunk::DBChunk;

/**
 * Integration test for all db functionality, things like linking, etc...
 */

use super::*;

fn init() -> DB {
	let mut db = DB::default();
	assert!(db.auth.new_user("nina", "444444").is_ok());
	// assert!(db.auth.new_user("john", "333333").is_ok());
	
	let mut chunk:DBChunk = ("# Todo \n").into();
	let mut id = chunk.chunk().id.clone();
	assert!(db.set_chunk(chunk, "nina").is_ok());
	// assert!(db
	// 	.set_chunk(("# Chores -> Todo\n - Vaccum\naccess: john r", "nina").into(), "nina")
	// 	.is_ok());

	// assert!(db.set_chunk(("# Todo \n", "john").into(), "john").is_ok());
	// assert!(db.set_chunk(("# Groceries -> todo", "john").into(), "john").is_ok());
	// assert!(db
	// 	.set_chunk(("# Work Stuff -> todo\nshare: nina write", "john").into(), "john")
	// 	.is_ok());

	db
}

#[test]
fn linking() {
	let mut db = init();
	
	{
		let all = db.chunks.values().map(|v|v.read().unwrap()).collect::<Vec<_>>();
		// println!("{all:?}");
	}
	db.link_all();
	{
		let all = db.chunks.values().map(|v|v.read().unwrap()).collect::<Vec<_>>();
		// println!("{all:?}");
	}
}
