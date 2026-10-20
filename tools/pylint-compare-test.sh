#!/usr/bin/env sh
# Self-check of STRIP=1 on a fixture: a "# pylint:" inside a string survives,
# a real pragma goes, and line numbers hold. The fake BIN saves the copy's file.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "$work"' EXIT

fail() {
	echo "pylint-compare-test: $*" >&2
	exit 1
}

code="$work/code"
mkdir "$code"
cat >"$code/mod.py" <<'EOF'
X = "# pylint: disable-next=unused-variable"
Y = 1  # pylint: disable=unused-variable
# pylint: disable=missing-docstring
Z = """
# pylint: disable=too-many-lines
"""
EOF
git -C "$code" init -q
git -C "$code" add mod.py

printf '#!/bin/sh\ncat mod.py >"%s/seen"\n' "$work" >"$work/bin"
chmod +x "$work/bin"
: >"$work/pylint.out"
STRIP=1 BIN="$work/bin" PYLINT_OUT="$work/pylint.out" CODE="$code" \
	"$root/tools/pylint-compare.sh" >/dev/null || fail "pylint-compare failed"

cat >"$work/want" <<'EOF'
X = "# pylint: disable-next=unused-variable"
Y = 1

Z = """
# pylint: disable=too-many-lines
"""
EOF
cmp -s "$work/want" "$work/seen" || {
	diff "$work/want" "$work/seen" >&2
	fail "the stripped copy is not the expected one"
}
echo "pylint-compare-test: ok"
