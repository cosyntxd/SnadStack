#!/bin/sh 

set -e

if [ "$(basename "$PWD")" = "web" ]; then
    echo "Error: The current directory can not be 'web'." >&2
    exit 1
fi

if [ "${1:-}" = "--release" ]; then
    build="web-release";
    profile="web-release";
else
    build="debug";
    profile="dev";
fi

cargo build --target wasm32-unknown-unknown --profile $profile

path="$(dirname "$0")/../target/wasm32-unknown-unknown/${build}/snad_stack.wasm"
wasm-bindgen --out-dir web/ --target web "$path"

if [ "$build" = "web-release" ]; then
    wasm-opt -O4 -o web/snad_stack_bg.wasm web/snad_stack_bg.wasm
fi

python3 -m http.server -d web
