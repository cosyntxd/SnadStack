#!/bin/sh
set -e

if [ -z "$1" ]; then
  cargo bench --target x86_64-pc-windows-gnu --no-run
  BENCHES=""
  for f in benches/*.rs; do
    bname=$(basename "$f" .rs)
    BENCHES="$BENCHES $bname"
  done
else
  cargo bench --bench "$1" --target x86_64-pc-windows-gnu --no-run
  BENCHES="$1"
fi

for b in $BENCHES; do
  EXE_PATH=$(ls -t target/x86_64-pc-windows-gnu/release/deps/${b}-*.exe 2>/dev/null | head -n 1)
  if [ -n "$EXE_PATH" ]; then
    WIN_PATH=$(wslpath -w "$EXE_PATH")
    cmd.exe /c "$WIN_PATH" --bench
  else
    echo "Could not find executable for benchmark: $b"
  fi
done