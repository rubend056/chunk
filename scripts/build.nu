#!/bin/nu

use env.nu *
use stop.nu *

export def build [] {
	# Exit if not in this directory
	if (pwd|lines).0 != "/home/rubend/p/chunk-app" {exit}

	# Just to make sure
	stop
	
	load_regex
	open prod.toml | load-env

	# Create output dirs
	rm -rf container/dist
	mkdir container/dist/web

	# Build server
	cargo build --release -Z unstable-options --out-dir container/dist

	# Build webapp
	enter web
		# Remove cache/build dirs
		rm -rf dist .parcel-cache
		# Build optimized
		yarn parcel build --public-url /web --no-source-maps
	exit

	# Copy webapp to output
	cp -r web/dist/* container/dist/web/
	rm -f container/dist/web/*.map
}

def main [] {
	build
}