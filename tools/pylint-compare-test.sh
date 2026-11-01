#!/usr/bin/env sh
# Self-check of STRIP=1 on a fixture: a "# pylint:" inside a string survives,
# a real pragma goes (also after other text in its comment), and line numbers
# hold; PY_VERSION lands in the copy's one [MASTER] section, never a second one.
# The fake BIN saves the copy's files.
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
W = 2  # ruff: ignore[x] pylint: disable=unused-variable
# pylint: disable=missing-docstring
Z = """
# pylint: disable=too-many-lines
"""
EOF
printf '  [MASTER]\njobs=1\n' >"$code/.pylintrc"
git -C "$code" init -q
git -C "$code" add mod.py

printf '#!/bin/sh\ncat mod.py >"%s/seen"; cat .pylintrc >"%s/rc"\n' "$work" "$work" >"$work/bin"
chmod +x "$work/bin"
: >"$work/pylint.out"
PY_VERSION=3.14 STRIP=1 BIN="$work/bin" PYLINT_OUT="$work/pylint.out" CODE="$code" \
	"$root/tools/pylint-compare.sh" >/dev/null || fail "pylint-compare failed"

cat >"$work/want" <<'EOF'
X = "# pylint: disable-next=unused-variable"
Y = 1
W = 2

Z = """
# pylint: disable=too-many-lines
"""
EOF
cmp -s "$work/want" "$work/seen" || {
	diff "$work/want" "$work/seen" >&2
	fail "the stripped copy is not the expected one"
}
[ "$(grep -ci '^[[:space:]]*\[MASTER\]' "$work/rc")" -eq 1 ] || fail "the copy's .pylintrc has not exactly one [MASTER]"
grep -q '^py-version=3.14$' "$work/rc" || fail "PY_VERSION did not reach the copy's .pylintrc"
echo "pylint-compare-test: ok"
