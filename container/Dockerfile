from archlinux

workdir /server

copy ./dist ./

expose 4000

env RUST_LOG=info  CACHE_PATH=data/cache.json  DB_PATH=data/db.json DB_BACK_FOLDER=backup

cmd ["./chunk-app"]