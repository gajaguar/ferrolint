#!/usr/bin/env sh
# Compare ferrolint's findings with Pylint's on CODE, one normalized
# "path:line:Pxxxx" per finding: build a configuration from every compiled
# plugin, run both linters, and print the findings each side has that the
# other has not. PYLINT_OUT, when it names an existing file, is reused
# instead of running Pylint, which takes ~106 s on a codebase; a missing one is
# written, so the slow run happens once. IGNORE is a comma-separated list
# of ferrolint rule codes the configuration ignores. PY_VERSION, when set, is
# the configuration's py-version, like "3.14". PYLINT is the Pylint
# command (default: pylint), run on CODE with its own .pylintrc.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fail() {
	echo "pylint-compare: $*" >&2
	exit 1
}

[ -n "${CODE:-}" ] || fail "set CODE to the directory to validate"
[ -d "$CODE" ] || fail "CODE is not a directory: $CODE"
: "${PYLINT:=pylint}"

bin=""
for kind in debug release; do
	if [ -x "$root/target/$kind/ferrolint" ]; then
		bin="$root/target/$kind/ferrolint"
		break
	fi
done
[ -n "$bin" ] || fail "no ferrolint binary under $root/target; run: make build"

# `ignore` belongs at the root of the configuration, before the first
# `[[plugins]]` table, where TOML would otherwise nest it inside the table.
config="$work/ferrolint.toml"
: >"$config"
[ -z "${PY_VERSION:-}" ] || printf 'py-version = "%s"\n' "$PY_VERSION" >>"$config"
if [ -n "${IGNORE:-}" ]; then
	list=""
	for code in $(printf '%s\n' "$IGNORE" | tr ',' ' '); do
		list="$list\"$code\", "
	done
	printf 'ignore = [%s]\n' "${list%, }" >>"$config"
fi
printf '\n' >>"$config"
for wasm in "$root"/target/wasm32-wasip2/release/ferrolint_plugin_*.wasm; do
	[ -f "$wasm" ] || fail "no plugins under $root/target/wasm32-wasip2/release; run: make plugins"
	case "$(basename "$wasm")" in
	ferrolint_plugin_test_* | test_*) continue ;;
	esac
	{
		printf '[[plugins]]\n'
		printf 'path = "%s"\n' "$wasm"
		printf 'sha256 = "%s"\n\n' "$(shasum -a 256 "$wasm" | cut -d' ' -f1)"
	} >>"$config"
done

# Both linters run with CODE as the working directory, so their paths are
# relative to it and Pylint picks up CODE's own .pylintrc.
cd "$CODE"
set +e
"$bin" --no-cache --config "$config" . >"$work/ferrolint.out"
code=$?
set -e
[ "$code" -le 1 ] || fail "ferrolint exited $code on CODE"

if [ -n "${PYLINT_OUT:-}" ] && [ -f "$PYLINT_OUT" ]; then
	pylint_out="$PYLINT_OUT"
else
	pylint_out="${PYLINT_OUT:-$work/pylint.out}"
	set +e
	# Unquoted: PYLINT may be several words, e.g. "poetry run pylint".
	$PYLINT --msg-template='{path}:{line}:{msg_id}' . >"$pylint_out" 2>"$work/pylint.err"
	code=$?
	set -e
	# A crash (a missing DJANGO_SETTINGS_MODULE, say) exits 1 with no findings,
	# which would read as parity; a rating line proves Pylint finished.
	if ! grep -q 'has been rated at' "$work/pylint.err"; then
		rm -f "$pylint_out"
		sed 's/^/  /' "$work/pylint.err" | tail -5 >&2
		fail "pylint did not finish; set its environment in PYLINT, e.g. PYLINT=\"env DJANGO_SETTINGS_MODULE=app.settings pylint\""
	fi
	# 32 is Pylint's usage error; below it the bits only count findings.
	if [ "$code" -ge 32 ]; then
		rm -f "$pylint_out"
		fail "pylint exited $code on CODE"
	fi
fi

# Pylint duplicates a message line at times; both sides collapse to the set
# of findings, since parity asks which findings each side has at all.
sed -nE 's/^([^:]+):([0-9]+):[0-9]+: [a-z]+ [A-Z0-9]+ \(([A-Z][0-9]{4})[,)].*/\1:\2:\3/p' \
	"$work/ferrolint.out" | LC_ALL=C sort -u >"$work/ferrolint.norm"
# shortcut: pylint . lints git-ignored files that ferrolint's walk skips; list
# the files instead when comparing a tree with generated code.
grep -E '^[^:]+\.py:[0-9]+:[A-Z][0-9]{4}$' "$pylint_out" | sed 's|^\./||' |
	LC_ALL=C sort -u >"$work/pylint.norm"

comm -23 "$work/ferrolint.norm" "$work/pylint.norm" >"$work/solo-ferrolint"
comm -13 "$work/ferrolint.norm" "$work/pylint.norm" >"$work/solo-pylint"
only_ferrolint="$(wc -l <"$work/solo-ferrolint" | tr -d ' ')"
only_pylint="$(wc -l <"$work/solo-pylint" | tr -d ' ')"

if [ "$only_ferrolint" -gt 0 ]; then
	echo "solo-ferrolint: $only_ferrolint"
	sed 's/^/  /' "$work/solo-ferrolint"
fi
if [ "$only_pylint" -gt 0 ]; then
	echo "solo-pylint: $only_pylint"
	sed 's/^/  /' "$work/solo-pylint"
fi

total=$((only_ferrolint + only_pylint))
if [ "$total" -eq 0 ]; then
	agree="$(comm -12 "$work/ferrolint.norm" "$work/pylint.norm" | wc -l | tr -d ' ')"
	echo "pylint-compare: ok ($agree findings agree)"
else
	echo "pylint-compare: $total differences"
	exit 1
fi
