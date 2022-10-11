#!/bin/nu

rm -r dist

cargo build --release -Z unstable-options --out-dir dist
mkdir dist/web

cd web
rm -r dist
yarn build
cd ..

cp -r web/dist/* dist/web/