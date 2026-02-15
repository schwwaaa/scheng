#!/usr/bin/env bash
set -euo pipefail

FORBIDDEN_FILE="ci/forbidden_deps.txt"

CONTRACT_CRATES=(
  "scheng-core"
  "scheng-graph"
  "scheng-runtime"
)

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found"
  exit 2
fi

FORBIDDEN=()
while IFS= read -r line; do
  line="$(echo "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  [[ -z "$line" ]] && continue
  [[ "$line" == \#* ]] && continue
  FORBIDDEN+=("$line")
done < "$FORBIDDEN_FILE"

fail=0

for crate in "${CONTRACT_CRATES[@]}"; do
  echo "== checking forbidden deps for: $crate =="

  TREE="$(cargo tree -p "$crate" 2>/dev/null || true)"
  if [[ -z "$TREE" ]]; then
    echo "WARNING: cargo tree returned nothing for $crate (crate missing?)"
    continue
  fi

  for dep in "${FORBIDDEN[@]}"; do
    if echo "$TREE" | grep -Eq "(^|[[:space:]])${dep}([[:space:]]|$)"; then
      echo "ERROR: $crate depends (directly or transitively) on forbidden crate: $dep"
      fail=1
    fi
  done
done

if [[ $fail -ne 0 ]]; then
  echo ""
  echo "Forbidden dependency gate FAILED."
  echo "Move platform/app deps out of contract crates."
  exit 1
fi

echo "Forbidden dependency gate OK."
