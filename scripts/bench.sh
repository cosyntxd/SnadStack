#!/bin/bash
set -e

if [ -z "$1" ]; then
  cargo bench --target x86_64-pc-windows-gnu --no-run
  for f in benches/*.rs; do benches+=("$(basename "$f" .rs)"); done
else
  cargo bench --bench "$1" --target x86_64-pc-windows-gnu --no-run
  benches=("$1")
fi

for b in "${benches[@]}"; do
  cmd.exe /c "$(wslpath -w "$(ls -t target/x86_64-pc-windows-gnu/release/deps/${b}-*.exe | head -n 1)")" --bench
done
