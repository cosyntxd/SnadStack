# Builds the wasm file for the web
cargo build --target wasm32-unknown-unknown --release
# Generates necessary bindings
wasm-bindgen --out-dir web/ --target web target/wasm32-unknown-unknown/release/snad_stack.wasm
# Optimizes the .wasm file
wasm-opt -O4 -o web/snad_stack_bg.wasm web/snad_stack_bg.wasm
# Serve the files on localhost:8000
python3 -m http.server -d web