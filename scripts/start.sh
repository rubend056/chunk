#!/bin/sh

cd web
	# rm -rf .parcel-cache dist
	rm -f build.log;nohup yarn parcel watch --public-url /web --log-level warn &>start.log &
cd ..
rm -f build.log;nohup cargo watch -w src -qN -x r &>start.log &

echo "Started cargo & parcel in background"

