#!/usr/bin/env bash

set -euo pipefail

repo_url="https://github.com/BotResources/br-llm-graph-rs"
crate="br-llm-graph"
readme="README.md"

version=$(grep -m1 '^version = ' Cargo.toml | sed -E 's/^version = "([^"]+)".*/\1/')
if [ -z "$version" ]; then
  echo "::error file=Cargo.toml::could not extract the package version" >&2
  exit 1
fi

if [ "$version" = "0.0.0" ]; then
  echo "✓ 0.0.0 scaffold — no tag has shipped, no self-pin to check"
  exit 0
fi

if [ ! -f "$readme" ]; then
  echo "::error::${readme} is missing" >&2
  exit 1
fi

expected_tag="v${version}"
expected_pin="package = \"${crate}\", tag = \"${expected_tag}\", version = \"${version}\""

if ! grep -qF "${expected_pin}" "$readme"; then
  echo "::error file=${readme}::the install pin must read ${expected_pin} (a tag-only git dep is a wildcard and cargo-deny denies it)" >&2
  exit 1
fi

fail=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  if ! grep -qF "${expected_pin}" <<<"$line"; then
    echo "::error file=${readme}::self-pin is stale: ${line}" >&2
    fail=1
  fi
done < <(grep -F "${repo_url}\"" "$readme" | grep -F 'git = ' || true)

if [ "$fail" -eq 0 ]; then
  echo "✓ ${readme}: self-pin on ${expected_tag} / ${version}"
fi

exit $fail
