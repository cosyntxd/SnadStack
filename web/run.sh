#!/bin/sh 

# Release or debug profile
if [ $1 = "--release" ]
then
    build="web-release";
    profile="--profile web-release";
else
    build="debug";
    profile="";
fi

# Wasm file location
path="target/wasm32-unknown-unknown/${build}/snad_stack.wasm"

# Builds the wasm file for the web
cargo build --target wasm32-unknown-unknown $profile

# Generates necessary bindings
wasm-bindgen --out-dir web/ --target web $path

# Optimizes the .wasm file
if [ $build = "web-release" ]
then
    wasm-opt -O4 -o web/snad_stack_bg.wasm web/snad_stack_bg.wasm
fi

# Serve the files on localhost:8000
python3 -m http.server -d web