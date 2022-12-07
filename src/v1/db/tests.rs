use crate::utils::DbError;

use super::*;
// use log::{info};
fn init() -> DB {
	let mut db = DB::default();
	assert!(db.new_user("nina".into(), "444444".into()).is_ok());
	assert!(db.new_user("john".into(), "333333".into()).is_ok());

	assert!(db.set_chunk("nina", (None, "# Todo".into())).is_ok());
	assert!(db
		.set_chunk("nina", (None, "# Chores -> Todo\n - Vaccum\naccess: john r".into()))
		.is_ok());

	assert!(db.set_chunk("john", (None, "# Todo".into())).is_ok());
	assert!(db.set_chunk("john", (None, "# Groceries -> todo".into())).is_ok());
	assert!(db
		.set_chunk("john", (None, "# Work Stuff -> todo\nshare: nina write".into()))
		.is_ok());

	db
}
use rand::distributions::{Alphanumeric, DistString};

#[test]
fn users() {
	let mut db = DB::default();
	assert_eq!(
		db.new_user("Nana3".into(), "1234".into()),
		Err(DbError::InvalidUsername),
		"Username characters invalid, only lowercase"
	);
	assert_eq!(
		db.new_user("Nana&".into(), "1234".into()),
		Err(DbError::InvalidUsername),
		"Username characters invalid, no special"
	);
	assert_eq!(
		db.new_user(":nana".into(), "1234".into()),
		Err(DbError::InvalidUsername),
		"Username characters invalid, no special"
	);
	assert_eq!(
		db.new_user("na".into(), "1234".into()),
		Err(DbError::InvalidUsername),
		"Username >= 3 in size"
	);
	assert_eq!(
		db.new_user("nan".into(), "12".into()),
		Err(DbError::InvalidPassword),
		"Password >= 6 in size"
	);
	assert_eq!(
		db.new_user("nan".into(), Alphanumeric.sample_string(&mut rand::thread_rng(), 70)),
		Err(DbError::InvalidPassword),
		"Password <= 64 in size"
	);
	assert!(db.new_user("nina".into(), "nina's pass".into()).is_ok());

	assert_eq!(db.users.len(), 1);

	assert!(db.login("nina", "wrong_pass").is_err(), "Password is wrong");
	assert!(db.login("nana", "wrong_pass").is_err(), "User nana doesn't exist");
	assert!(db.login("nina", "nina's pass").is_ok(), "Login success");
}

#[test]
fn chunks() {
	let mut db = init();
	// Checking chunk validation
	// assert!(db.set_chunk("nina", (None, "4444".into())).is_err());
	// assert!(db.set_chunk("nina", (None, "# -> jack".into())).is_err());
	// assert!(db.set_chunk("nina", (None, "#nack".into())).is_err());
	// assert!(db.set_chunk("nina", (None, "access: nomad read".into())).is_err());
	// Chunks no longer throw when badly formatted, this will probably be introduced later for certain trees.

	let nina_chores = db.get_chunk(Some("nina".into()), &"Chores".into()).unwrap();
	let john_work_stuff = db.get_chunk(Some("john".into()), &"Work Stuff".into()).unwrap();

	assert_eq!(db.get_notes("nina").len(), 3);
	assert_eq!(db.get_notes("john").len(), 4);

	assert!(db
		.set_chunk(
			"john",
			(
				Some(nina_chores.id.clone()),
				"# Chores -> Todo\n - Vaccum\naccess: john r".into()
			)
		)
		.is_err());
	assert!(db
		.set_chunk(
			"john",
			(Some(nina_chores.id.clone()), "# Chores -> Todo\n - Vaccum".into())
		)
		.is_err());

	assert!(
		db.set_chunk(
			"nina",
			(
				Some(john_work_stuff.id.clone()),
				"# Work Stu -> todo\nshare: nina write".into()
			)
		)
		.is_err(),
		"Nina has write access but can't change title, title is checked by _ref/title props in ChunkMeta"
	);
	let r = db.set_chunk(
		"nina",
		(
			Some(john_work_stuff.id.clone()),
			"# work stuff -> Todo\nshare: nina w".into(),
		),
	);
	assert!(r.is_err(), "Title Changed, write should fail'{r:?}'");
	let r = db.set_chunk(
		"nina",
		(
			Some(john_work_stuff.id.clone()),
			"# Work Stuff -> Todo\nshare: nina r".into(),
		),
	);
	assert!(r.is_err(), "Can't change access, fails'{r:?}'");
	let r = db.set_chunk(
		"nina",
		(
			Some(john_work_stuff.id.clone()),
			"# Work Stuff -> Todo\nCan change content :)\nshare: nina w".into(),
		),
	);
	assert!(
		r.is_ok(),
		"Can change content since nina has write access, succeeds'{r:?}'"
	);
}
#[test]
fn views() {
	// let db = init();
}
#[test]
fn delete() {
	let mut db = init();
	let john_work_stuff = db.get_chunk(Some("john".into()), &"Work Stuff".into()).unwrap();
	assert!(db.del_chunk(&"nina".into(), vec![john_work_stuff.id.clone()]).is_ok());
	// Is ok bc this will only delete nina's access to it, nothing more
}
#[test]
fn access() {}
