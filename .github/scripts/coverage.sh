#!/usr/bin/env bash
set -euo pipefail

# Test coverage of the workspace crates, measured with cargo-llvm-cov.
#
# The measure is kept as coverage-summary.json: per crate, the covered and
# total lines, regions and functions, in total and per file (paths relative to
# the crate). CI keeps it as the `coverage` artifact of every run; the release
# workflow attaches it to the GitHub release of each version, so a release's
# coverage can be compared with the previous one long after CI artifacts expire.

SCHEMA=1

usage() {
    cat >&2 <<'USAGE'
usage:
  coverage.sh measure <out-dir> [<crate> [<commit>]]
  coverage.sh normalize <llvm-export.json> <cargo-metadata.json> <commit> <tool>
  coverage.sh render <summary.json> [<reference-summary.json> <reference-label>]
  coverage.sh reference <branch> <out-dir>
  coverage.sh earlier <crate> <version>
  coverage.sh release <crate> <version> <commit>
USAGE
    exit 2
}

# Turn a `cargo llvm-cov report --json --summary-only` export into
# coverage-summary.json. Files outside the workspace members are dropped.
normalize() {
    [[ $# -eq 4 ]] || usage
    python3 - "$@" <<'PY'
import json, os, sys

export_file, metadata_file, commit, tool = sys.argv[1:5]
export = json.load(open(export_file))
metadata = json.load(open(metadata_file))
members = set(metadata["workspace_members"])
crates = sorted(
    ((os.path.dirname(p["manifest_path"]) + os.sep, p["name"]) for p in metadata["packages"] if p["id"] in members),
    key=lambda entry: len(entry[0]),
    reverse=True,
)
KINDS = ("lines", "regions", "functions")

def counts(summary):
    return {kind: {"covered": summary[kind]["covered"], "count": summary[kind]["count"]} for kind in KINDS}

result = {}
for data in export["data"]:
    for entry in data["files"]:
        path = entry["filename"]
        owner = next(((root, name) for root, name in crates if path.startswith(root)), None)
        if owner is None:
            continue
        root, name = owner
        crate = result.setdefault(name, {"totals": {k: {"covered": 0, "count": 0} for k in KINDS}, "files": {}})
        file_counts = counts(entry["summary"])
        crate["files"][path[len(root):]] = file_counts
        for kind in KINDS:
            crate["totals"][kind]["covered"] += file_counts[kind]["covered"]
            crate["totals"][kind]["count"] += file_counts[kind]["count"]

for crate in result.values():
    crate["files"] = dict(sorted(crate["files"].items()))
json.dump({"schema": int(os.environ["SCHEMA"]), "tool": tool, "commit": commit, "crates": dict(sorted(result.items()))},
          sys.stdout, indent=1)
print()
PY
}

# Markdown report of a summary, with the delta against a reference summary when
# one is given. A crate whose line coverage dropped is also reported as a
# workflow warning on stderr.
render() {
    [[ $# -eq 1 || $# -eq 3 ]] || usage
    python3 - "$@" <<'PY'
import json, sys

current = json.load(open(sys.argv[1]))
reference = json.load(open(sys.argv[2])) if len(sys.argv) > 2 else None
label = sys.argv[3] if len(sys.argv) > 3 else None
KINDS = ("lines", "regions", "functions")

def percent(c):
    return None if c["count"] == 0 else 100.0 * c["covered"] / c["count"]

def fmt(value):
    return "—" if value is None else "%.2f%%" % value

def delta(now, before):
    if now is None or before is None:
        return "—"
    d = now - before
    if abs(d) < 0.005:
        return "="
    return ("+%.2f" % d) if d > 0 else ("−%.2f" % -d)

out = []
for name, crate in current["crates"].items():
    ref = (reference or {}).get("crates", {}).get(name) if reference else None
    out.append("### Coverage — %s" % name)
    out.append("")
    if reference is None:
        out.append("No reference coverage to compare with.")
    elif ref is None:
        out.append("%s has no coverage of %s to compare with." % (label, name))
    else:
        out.append("Δ against %s (`%s`)." % (label, reference["commit"][:12]))
    out.append("")
    out.append("| | Covered | Total | % | Δ |")
    out.append("|---|---:|---:|---:|---:|")
    for kind in KINDS:
        c = crate["totals"][kind]
        before = percent(ref["totals"][kind]) if ref else None
        out.append("| %s | %d | %d | %s | %s |" % (kind.capitalize(), c["covered"], c["count"], fmt(percent(c)), delta(percent(c), before)))
    out.append("")

    rows = []
    for path, c in crate["files"].items():
        now = percent(c["lines"])
        was = ref["files"].get(path) if ref else None
        before = percent(was["lines"]) if was else None
        change = delta(now, before) if ref else "—"
        if ref and was is None:
            change = "new"
        rows.append((now if now is not None else 101.0, path, c["lines"], now, change))
    if ref:
        for path in sorted(set(ref["files"]) - set(crate["files"])):
            rows.append((102.0, path, None, None, "removed"))
    moved = sum(1 for row in rows if row[4] not in ("=", "—"))
    out.append("<details><summary>Line coverage per file (%d files%s)</summary>" % (
        len(crate["files"]), ", %d changed" % moved if ref else ""))
    out.append("")
    out.append("| File | Lines | % | Δ |")
    out.append("|---|---:|---:|---:|")
    for _, path, lines, now, change in sorted(rows):
        cells = "%d / %d" % (lines["covered"], lines["count"]) if lines else "—"
        out.append("| `%s` | %s | %s | %s |" % (path, cells, fmt(now), change))
    out.append("")
    out.append("</details>")
    out.append("")

    if ref:
        now, before = percent(crate["totals"]["lines"]), percent(ref["totals"]["lines"])
        if now is not None and before is not None and now - before <= -0.005:
            print("::warning::%s line coverage dropped from %.2f%% to %.2f%% against %s." % (name, before, now, label),
                  file=sys.stderr)

print("\n".join(out))
PY
}

# Measure the tests of one crate, or of the whole workspace, at a commit (HEAD
# by default). Writes <out-dir>/coverage-summary.json, lcov.info and html/.
ROOT=""
SRC=""
WORK=""
cleanup() {
    if [[ -n "$SRC" && "$SRC" != "$ROOT" ]]; then
        git -C "$ROOT" worktree remove --force "$SRC" 2>/dev/null || true
        git -C "$ROOT" worktree prune
    fi
    [[ -z "$WORK" ]] || rm -rf "$WORK"
}

measure() {
    [[ $# -ge 1 && $# -le 3 ]] || usage
    local out="$1" crate="${2:-}" commit="${3:-HEAD}" tool
    ROOT="$(git rev-parse --show-toplevel)"
    SRC="$ROOT"
    WORK="$(mktemp -d)"
    trap cleanup EXIT
    if [[ "$(git -C "$ROOT" rev-parse "${commit}^{commit}")" != "$(git -C "$ROOT" rev-parse HEAD)" ]]; then
        SRC="$WORK/tree"
        git -C "$ROOT" worktree add --detach "$SRC" "$commit" >/dev/null
    fi
    commit="$(git -C "$SRC" rev-parse HEAD)"

    local scope=(--workspace)
    [[ -z "$crate" ]] || scope=(-p "$crate")

    mkdir -p "$out"
    out="$(cd "$out" && pwd)"
    tool="$(cargo llvm-cov --version)"
    (
        cd "$SRC"
        cargo llvm-cov clean --workspace
        cargo llvm-cov "${scope[@]}" --all-targets --all-features --locked --no-report
        cargo llvm-cov report --json --summary-only --output-path "$WORK/export.json"
        cargo llvm-cov report --lcov --output-path "$out/lcov.info"
        cargo llvm-cov report --html --output-dir "$out"
        cargo metadata --format-version 1 --no-deps --locked >"$WORK/metadata.json"
    )
    normalize "$WORK/export.json" "$WORK/metadata.json" "$commit" "$tool" >"$out/coverage-summary.json"
    echo "coverage: ${crate:-workspace} at ${commit} -> ${out}/coverage-summary.json"
}

# Fetch the coverage artifact of the latest successful CI run pushed to a
# branch. Nothing is fetched (and nothing fails) when there is none yet.
reference() {
    [[ $# -eq 2 ]] || usage
    local branch="$1" out="$2" run_id
    : "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
    run_id="$(gh run list --repo "$GITHUB_REPOSITORY" --workflow ci.yml --branch "$branch" --event push \
        --status success --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
    if [[ -z "$run_id" ]]; then
        echo "::notice::coverage: no successful CI run on ${branch} yet — no reference to compare with."
        return 0
    fi
    if ! gh run download "$run_id" --repo "$GITHUB_REPOSITORY" --name coverage --dir "$out"; then
        echo "::notice::coverage: CI run ${run_id} on ${branch} kept no coverage artifact — no reference to compare with."
        return 0
    fi
    echo "coverage: reference from CI run ${run_id} on ${branch}"
}

# The tagged versions of a crate below a version, highest first.
earlier_versions() {
    local crate="$1" version="$2" tagged
    git tag --list "${crate}/v*" | sed "s|^${crate}/v||" | sort -V -r | while read -r tagged; do
        [[ "$tagged" != "$version" ]] || continue
        [[ "$(printf '%s\n%s\n' "$tagged" "$version" | sort -V | head -n 1)" == "$tagged" ]] || continue
        echo "$tagged"
    done
}

# Measure a released version at its commit and attach coverage-summary.json to
# its GitHub release; report the delta against the previous release that
# carries one. Re-running replaces the attached summary.
release() {
    [[ $# -eq 3 ]] || usage
    local crate="$1" version="$2" commit="$3"
    local tag="${crate}/v${version}" root out previous=""
    root="$(git rev-parse --show-toplevel)"
    out="$root/target/coverage/${crate}-${version}"
    : "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

    if ! gh release view "$tag" --repo "$GITHUB_REPOSITORY" >/dev/null 2>&1; then
        echo "::error::coverage: the GitHub release ${tag} does not exist — nothing to attach the coverage to." >&2
        return 1
    fi
    rm -rf "$out"
    measure "$out" "$crate" "$commit"
    gh release upload "$tag" "$out/coverage-summary.json" --repo "$GITHUB_REPOSITORY" --clobber
    echo "coverage: attached to ${tag}"

    local candidate
    while read -r candidate; do
        if gh release download "${crate}/v${candidate}" --repo "$GITHUB_REPOSITORY" \
            --pattern coverage-summary.json --dir "$out/previous" --clobber >/dev/null 2>&1; then
            previous="$candidate"
            break
        fi
    done < <(earlier_versions "$crate" "$version")

    local report="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
    if [[ -n "$previous" ]]; then
        render "$out/coverage-summary.json" "$out/previous/coverage-summary.json" "${crate} ${previous}" >>"$report"
    else
        render "$out/coverage-summary.json" >>"$report"
    fi
}

[[ $# -ge 1 ]] || usage
command="$1"
shift
export SCHEMA
case "$command" in
    measure | normalize | render | reference | release) "$command" "$@" ;;
    earlier) [[ $# -eq 2 ]] || usage; earlier_versions "$@" ;;
    *) usage ;;
esac
