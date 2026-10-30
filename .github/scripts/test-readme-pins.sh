#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/test-lib.sh"

URL="https://github.com/BotResources/br-llm-graph-rs"
SCRIPT="$SRC_SCRIPTS/check-readme-pins.sh"

in_dir() {
    local dir="$1"
    shift
    (cd "$dir" && "$@")
}

pin() { printf '%s = { git = "%s", package = "%s", tag = "%s/v%s", version = "%s" }\n' "$1" "$URL" "$1" "$1" "$2" "$2"; }

fixture() {
    local name="$1" contract_pin="$2" nodes_contract_pin="$3"
    local dir="$WORK/$name"
    mkdir -p "$dir/crates/br-agentic-contract" "$dir/crates/br-agentic-nodes"
    printf '[package]\nname = "br-agentic-contract"\nversion = "0.2.0"\n' >"$dir/crates/br-agentic-contract/Cargo.toml"
    printf '[package]\nname = "br-agentic-nodes"\nversion = "0.1.0"\n' >"$dir/crates/br-agentic-nodes/Cargo.toml"
    {
        printf '# lib\n\n## br-agentic-contract\n\n```toml\n[dependencies]\n'
        [[ -n "$contract_pin" ]] && printf '%s\n' "$contract_pin"
        pin br-graph-contract 0.1.0
        printf '```\n\n## br-agentic-nodes\n\n```toml\n[dependencies]\n'
        pin br-agentic-nodes 0.1.0
        printf '%s\n' "$nodes_contract_pin"
        printf '```\n'
    } >"$dir/README.md"
    echo "$dir"
}

echo "== a sibling snippet pinning the checked crate on another version is ignored"
dir=$(fixture sibling "$(pin br-agentic-contract 0.2.0)" "$(pin br-agentic-contract 0.1.0)")
expect 0 "br-agentic-contract self-pin on br-agentic-contract/v0.2.0" "contract passes while nodes pins contract 0.1.0" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract
expect 0 "br-agentic-nodes self-pin on br-agentic-nodes/v0.1.0" "the sibling own snippet is checked on its own" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-nodes

echo "== the own snippet with a stale pin still fails"
dir=$(fixture stale "$(pin br-agentic-contract 0.1.0)" "$(pin br-agentic-contract 0.2.0)")
expect 1 "install pin must read" "own pin on 0.1.0 while the crate is 0.2.0" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract

echo "== a stale second self-pin line inside the own snippet fails"
dir=$(fixture stale-line "$(pin br-agentic-contract 0.2.0)" "$(pin br-agentic-contract 0.1.0)")
sed -i.bak 's|^br-graph-contract = .*|br-agentic-contract = { git = "'"$URL"'", package = "br-agentic-contract", tag = "br-agentic-contract/v0.1.0", version = "0.1.0" }|' "$dir/README.md"
expect 1 "self-pin is stale" "a stale self-pin line in the own snippet" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract

echo "== a missing own snippet fails closed"
dir=$(fixture missing "" "$(pin br-agentic-contract 0.2.0)")
expect 1 "no install snippet of br-agentic-contract found" "no own snippet while a sibling pins the right version" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract
printf '# lib\n' >"$dir/README.md"
expect 1 "no install snippet" "a README without any snippet" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract
rm "$dir/README.md"
expect 1 "README.md is missing" "no README" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract

echo "== a 0.0.0 scaffold has no pin to check"
dir=$(fixture scaffold "" "")
printf '[package]\nname = "br-agentic-contract"\nversion = "0.0.0"\n' >"$dir/crates/br-agentic-contract/Cargo.toml"
expect 0 "scaffold" "0.0.0 passes" \
    in_dir "$dir" bash "$SCRIPT" br-agentic-contract

summary
