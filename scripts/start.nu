
use env.nu *
use stop.nu *

export def-env setup_dev [] {
	load_regex
	let-env WEB_DIST = $"($env.PWD)/web/dist/web"
	let-env BACKEND_DIST = $"($env.PWD)/web/dist/backend"
	open dev.toml | load-env
}

export def start [] {
	stop
	setup_dev
	
	/bin/env scripts/start.sh
}

export def run [] {
	stop 
	setup_dev
	
	cargo run
}

export def test [] {
	stop 
	setup_dev
	
	cargo test
}

export def check [] {
	stop 
	setup_dev
	
	cargo check
}