#!/bin/nu

use build.nu *

export def deploy [] {
	
	build

	do -i {docker stop chunk_s}
	do -i {docker rm chunk_s}
	docker build -t chunk ./container
	docker volume create -d local chunk_data
	docker run -dp 4500:4000 -v chunk_data:/server/data --name chunk_s chunk
}


def main [] {
	deploy
}