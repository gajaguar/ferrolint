#!/usr/bin/env bash
# Smoke-test a ferrolint binary the way a release must start on its target
# with nothing else installed: version, help, configuration validation,
# a clean run, a run with findings, and the start-up budget.
set -euo pipefail

bin="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

fail() {
	echo "smoke: $*" >&2
	exit 1
}

exit_code() {
	set +e
	"$@" >/dev/null 2>&1
	local code=$?
	set -e
	echo "$code"
}

"$bin" --version | grep -q '^ferrolint ' || fail "--version"
"$bin" --help | grep -q -- '--format' || fail "--help"

printf 'select = [\n' >ferrolint.toml
[ "$(exit_code "$bin")" -eq 2 ] || fail "a malformed configuration must exit 2"
printf 'select = ["NOPE1"]\n' >ferrolint.toml
[ "$(exit_code "$bin")" -eq 2 ] || fail "an unknown rule must exit 2"
rm ferrolint.toml

printf 'X = 1\n' >clean.py
[ "$(exit_code "$bin" clean.py)" -eq 0 ] || fail "a clean file must exit 0"
printf 'X = 1  # noqa\n' >finding.py
[ "$(exit_code "$bin" finding.py)" -eq 1 ] || fail "a finding must exit 1"

# A cargo-built binary has no bundled plugins; the acceptance test sets this.
if [ -z "${SMOKE_UNBUNDLED:-}" ]; then
	printf 'import os\n' >unused.py
	out="$("$bin" unused.py || true)"
	case "$out" in *IMP001* | *W0611*) ;; *) fail "the bundled plugins must report an unused import" ;; esac
	[ "$(exit_code "$bin" unused.py)" -eq 1 ] || fail "an unused import must exit 1"
fi

first_parse="$("$bin" --stats --no-cache clean.py 2>&1 >/dev/null | sed -n 's/.*first-parse-ms=\([0-9.]*\).*/\1/p')"
awk -v ms="$first_parse" 'BEGIN { exit !(ms != "" && ms < 100) }' || fail "first parse took ${first_parse:-?} ms"

echo "smoke: ok ($first_parse ms to the first parse)"
