#!/usr/bin/env bash
set -euo pipefail

version=$(grep -m1 '^version = ' Cargo.toml | sed -E 's/^version = "([^"]+)".*/\1/')
if [ -z "$version" ]; then
  echo "::error file=Cargo.toml::could not extract the package version" >&2
  exit 1
fi

changelog="CHANGELOG.md"
if [ ! -f "$changelog" ]; then
  echo "::error::package version ${version} but ${changelog} is missing" >&2
  exit 1
fi

if [ "$version" = "0.0.0" ]; then
  if grep -qE "^## 0\.0\.0( |\$)" "$changelog"; then
    echo "::error file=${changelog}::0.0.0 is the unreleased scaffold version and must not carry a release heading" >&2
    exit 1
  fi
  echo "✓ 0.0.0 scaffold — unreleased, nothing to tag"
  exit 0
fi

if ! grep -qE "^## ${version}( |\$)" "$changelog"; then
  echo "::error file=${changelog}::v${version} has no '## ${version}' entry. Add a plain '## ${version}' section before merging." >&2
  exit 1
fi

echo "✓ v${version}"
