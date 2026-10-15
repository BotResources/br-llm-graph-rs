#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/test-lib.sh"

COVERAGE="$SRC_SCRIPTS/coverage.sh"
TREE="$WORK/tree"

# A cargo metadata of two workspace members, br-alpha and br-alpha-extra (whose
# directory starts like br-alpha's), and an llvm-cov export over their files
# plus one file of a dependency, which is not the workspace's to measure.
fixture_metadata() {
    python3 - "$TREE" >"$WORK/metadata.json" <<'PY'
import json, sys
tree = sys.argv[1]
def pkg(name):
    return {"id": "path+file://%s/crates/%s#0.1.0" % (tree, name), "name": name,
            "manifest_path": "%s/crates/%s/Cargo.toml" % (tree, name)}
packages = [pkg("br-alpha"), pkg("br-alpha-extra")]
json.dump({"packages": packages, "workspace_members": [p["id"] for p in packages]}, sys.stdout)
PY
}

fixture_export() {
    python3 - "$TREE" "$@" >"$WORK/export.json" <<'PY'
import json, sys
tree, specs = sys.argv[1], sys.argv[2:]
def summary(covered, count):
    s = lambda c, n: {"covered": c, "count": n, "percent": 0}
    return {"lines": s(covered, count), "regions": s(covered * 2, count * 2), "functions": s(1, 1),
            "branches": s(0, 0), "instantiations": s(1, 1)}
files = []
for spec in specs:
    path, covered, count = spec.split(":")
    filename = path if path.startswith("/") else "%s/%s" % (tree, path)
    files.append({"filename": filename, "summary": summary(int(covered), int(count))})
json.dump({"type": "llvm.coverage.json.export", "version": "2.0.1", "data": [{"files": files, "totals": {}}]}, sys.stdout)
PY
}

field() { python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); [d := d[k] for k in sys.argv[2:]]; print(json.dumps(d, sort_keys=True))' "$@"; }

echo "== normalize: per crate, per file, relative paths, dependencies dropped"
fixture_metadata
fixture_export crates/br-alpha/src/lib.rs:8:10 crates/br-alpha/src/graph.rs:1:10 \
    crates/br-alpha-extra/src/lib.rs:3:4 /home/runner/.cargo/registry/serde/src/lib.rs:5:9
bash "$COVERAGE" normalize "$WORK/export.json" "$WORK/metadata.json" abc123 "cargo-llvm-cov 0.6.0" >"$WORK/summary.json"
expect_equal "$(field "$WORK/summary.json" schema)" 1 "the summary carries its schema version"
expect_equal "$(field "$WORK/summary.json" commit)" '"abc123"' "the summary names the measured commit"
expect_equal "$(field "$WORK/summary.json" tool)" '"cargo-llvm-cov 0.6.0"' "the summary names the tool"
expect_equal "$(python3 -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))["crates"]))' "$WORK/summary.json")" \
    "br-alpha br-alpha-extra" "one entry per workspace crate, the dependency's file dropped"
expect_equal "$(field "$WORK/summary.json" crates br-alpha totals lines)" '{"count": 20, "covered": 9}' "lines add up over the crate's files"
expect_equal "$(field "$WORK/summary.json" crates br-alpha totals regions)" '{"count": 40, "covered": 18}' "regions add up over the crate's files"
expect_equal "$(field "$WORK/summary.json" crates br-alpha-extra files src/lib.rs lines)" '{"count": 4, "covered": 3}' \
    "a file of br-alpha-extra is not taken for br-alpha (longest crate directory wins)"
expect_equal "$(python3 -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))["crates"]["br-alpha"]["files"]))' "$WORK/summary.json")" \
    "src/graph.rs src/lib.rs" "file paths are relative to the crate, sorted"

echo "== render without a reference"
run bash "$COVERAGE" render "$WORK/summary.json"
expect_equal "$RC" 0 "render succeeds"
grep -qF "### Coverage — br-alpha" <<<"$OUT" && verdict yes "one section per crate" || verdict no "one section per crate"
grep -qF "| Lines | 9 | 20 | 45.00% | — |" <<<"$OUT" && verdict yes "totals with their percentage, no delta" || verdict no "totals with their percentage, no delta"
grep -qF "No reference coverage to compare with." <<<"$OUT" && verdict yes "says there is nothing to compare with" || verdict no "says there is nothing to compare with"
[[ "$(grep -n 'src/graph.rs' <<<"$OUT" | cut -d: -f1)" -lt "$(grep -n '`src/lib.rs`' <<<"$OUT" | head -1 | cut -d: -f1)" ]] \
    && verdict yes "the least covered file comes first" || verdict no "the least covered file comes first"
grep -qF "::warning::" <<<"$OUT" && verdict no "no warning without a reference" || verdict yes "no warning without a reference"

echo "== render against a reference"
cp "$WORK/summary.json" "$WORK/reference.json"
fixture_export crates/br-alpha/src/lib.rs:8:10 crates/br-alpha/src/graph.rs:1:20 crates/br-alpha/src/new.rs:2:2 \
    crates/br-alpha-extra/src/lib.rs:4:4
bash "$COVERAGE" normalize "$WORK/export.json" "$WORK/metadata.json" def456 "cargo-llvm-cov 0.6.0" >"$WORK/summary.json"
run bash "$COVERAGE" render "$WORK/summary.json" "$WORK/reference.json" "main"
expect_equal "$RC" 0 "render succeeds"
grep -qF 'Δ against main (`abc123`).' <<<"$OUT" && verdict yes "names the reference and its commit" || verdict no "names the reference and its commit"
grep -qF "| Lines | 11 | 32 | 34.38% | −10.62 |" <<<"$OUT" && verdict yes "a drop shows as a negative delta" || verdict no "a drop shows as a negative delta"
grep -qF '| `src/graph.rs` | 1 / 20 | 5.00% | −5.00 |' <<<"$OUT" && verdict yes "per-file delta" || verdict no "per-file delta"
grep -qF '| `src/new.rs` | 2 / 2 | 100.00% | new |' <<<"$OUT" && verdict yes "a new file is marked new" || verdict no "a new file is marked new"
grep -qF '| `src/lib.rs` | 8 / 10 | 80.00% | = |' <<<"$OUT" && verdict yes "an unchanged file is marked =" || verdict no "an unchanged file is marked ="
grep -qF "| Lines | 4 | 4 | 100.00% | +25.00 |" <<<"$OUT" && verdict yes "a rise shows as a positive delta" || verdict no "a rise shows as a positive delta"
grep -qF "::warning::br-alpha line coverage dropped from 45.00% to 34.38% against main." <<<"$OUT" \
    && verdict yes "a drop raises a workflow warning" || verdict no "a drop raises a workflow warning"
grep -qF "::warning::br-alpha-extra" <<<"$OUT" && verdict no "a rise raises no warning" || verdict yes "a rise raises no warning"

echo "== render: a removed file and a crate the reference does not know"
fixture_export crates/br-alpha/src/lib.rs:8:10 crates/br-alpha-extra/src/lib.rs:4:4
bash "$COVERAGE" normalize "$WORK/export.json" "$WORK/metadata.json" fed789 "cargo-llvm-cov 0.6.0" >"$WORK/summary.json"
python3 -c '
import json, sys
d = json.load(open(sys.argv[1])); del d["crates"]["br-alpha-extra"]; json.dump(d, open(sys.argv[1], "w"))' "$WORK/reference.json"
run bash "$COVERAGE" render "$WORK/summary.json" "$WORK/reference.json" "br-alpha 0.3.0"
grep -qF '| `src/graph.rs` | — | — | removed |' <<<"$OUT" && verdict yes "a removed file is marked removed" || verdict no "a removed file is marked removed"
grep -qF "br-alpha 0.3.0 has no coverage of br-alpha-extra to compare with." <<<"$OUT" \
    && verdict yes "a crate absent from the reference is said so" || verdict no "a crate absent from the reference is said so"

echo "== earlier versions: the tagged versions below one, highest first"
mkdir -p "$WORK/tags"
git -C "$WORK/tags" init -q
git -C "$WORK/tags" commit -q --allow-empty -m init
for tag in br-alpha/v0.1.0 br-alpha/v0.2.0 br-alpha/v0.2.1 br-alpha/v0.10.0 br-beta/v0.4.0 v0.9.0; do
    git -C "$WORK/tags" tag "$tag"
done
earlier() { (cd "$WORK/tags" && bash "$COVERAGE" earlier "$@" | tr '\n' ' '); }
expect_equal "$(earlier br-alpha 0.5.0)" "0.2.1 0.2.0 0.1.0 " "0.5.0 comes after 0.2.1, 0.2.0, 0.1.0 — not 0.10.0, not another crate's tags"
expect_equal "$(earlier br-alpha 0.2.1)" "0.2.0 0.1.0 " "the version itself is not its own predecessor"
expect_equal "$(earlier br-alpha 0.1.0)" "" "the first version has none"

echo "== usage"
expect 2 "usage:" "an unknown subcommand prints the usage" bash "$COVERAGE" frobnicate
expect 2 "usage:" "render with a reference but no label prints the usage" bash "$COVERAGE" render a b

summary
