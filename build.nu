#!/bin/nu

# Exit if not in this directory
if (pwd|lines).0 != "/home/rubend/p/chunk-app" {exit}

# Create output dirs
rm -rf container/dist
mkdir container/dist/web

# Build server
cargo build --release -Z unstable-options --out-dir container/dist

# Build webapp
enter web
	rm -rf dist .parcel-cache
	yarn parcel build --public-url /web --no-source-maps
exit

# Copy webapp to output
cp -r web/dist/* container/dist/web/