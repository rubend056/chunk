from archlinux

workdir /server

copy ./dist ./

expose 4000

env RUST_LOG=info
env DB_PATH=data/chunks.json

cmd ["./chunk-app"]