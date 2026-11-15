#!/usr/bin/env bash
set -euo pipefail
SOURCE_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for tool in cmake ninja pkg-config cargo rustc mold; do
    command -v "$tool" >/dev/null 2>&1 || { echo "Missing build tool: $tool. See README.md." >&2; exit 1; }
done
cmake -S "$SOURCE_DIR" -B "$SOURCE_DIR/build" -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build "$SOURCE_DIR/build" --parallel 2
echo "Built: $SOURCE_DIR/build/linux-scripts-dock"
