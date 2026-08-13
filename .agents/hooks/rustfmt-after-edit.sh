#!/usr/bin/env bash
# Format edited Rust files after agent edits. Fail-open if tools are missing.
set -u

input=$(cat || true)
file_path=""

if command -v python3 >/dev/null 2>&1; then
  file_path=$(printf '%s' "$input" | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
print(data.get("file_path") or "")
' 2>/dev/null || true)
fi

if [[ -z "${file_path}" || "${file_path}" != *.rs ]]; then
  exit 0
fi

if [[ ! -f "${file_path}" ]]; then
  exit 0
fi

if command -v rustfmt >/dev/null 2>&1; then
  rustfmt --edition 2021 "${file_path}" >/dev/null 2>&1 || true
  exit 0
fi

if command -v cargo >/dev/null 2>&1; then
  cargo fmt -- "${file_path}" >/dev/null 2>&1 || true
fi

exit 0
