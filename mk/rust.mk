CARGO := cargo
CARGO_DENY_VERSION := 0.20.2
# The crates that build for the plugin target: every workspace member but
# the host-only linter and its core types.
WASM_TARGET := wasm32-wasip2
WASM_PACKAGES := --workspace --exclude ferrolint --exclude ferrolint-core
# The Pylint the sample plugin's fixtures are recorded against. Pinned: a
# checker's verdict changes between versions.
PYLINT ?= uvx pylint@4.1.3
# The wheel tooling, pinned like Pylint.
MATURIN ?= uvx maturin@1.15.0
WHEEL ?= uvx wheel@0.48.0
SAMPLE_FIXTURES := plugins/unused-import/fixtures
PYLINT_W0611 = $(PYLINT) --disable=all --enable=unused-import,unused-variable --score=n --persistent=n \
	--msg-template '{line}:{column}:{msg_id}:{symbol}:{msg}'

LANG_INSTALL_TARGETS += install-rust
LANG_CHECK_TARGETS   += clippy fmt-check deny
LANG_FIX_TARGETS     += fmt
LANG_TEST_TARGETS    += cargo-test

##@ Rust

install-rust: ## Fetch the crate's dependencies and install the pinned cargo-deny
	$(CARGO) fetch --locked
	$(CARGO) install --locked cargo-deny@$(CARGO_DENY_VERSION)

clippy: ## Lint with Clippy for the host and the plugin target, treating warnings as errors
	$(CARGO) clippy --locked --all-targets -- -D warnings
	$(CARGO) clippy --locked --target $(WASM_TARGET) $(WASM_PACKAGES) -- -D warnings

fmt: ## Format code with rustfmt in place — accepts FILES="..."
	$(CARGO) fmt -- $(FILES)

fmt-check: ## Check formatting with rustfmt, without writing — accepts FILES="..."
	$(CARGO) fmt --check -- $(FILES)

deny: ## Check dependencies for advisories, licenses and sources with cargo-deny
	$(CARGO) deny --locked check

cargo-test: ## Run the test suite with cargo test
	$(CARGO) test --locked

build: ## Build the crate into target/
	$(CARGO) build --locked

plugins: ## Build the workspace plugins as WASM components into target/wasm32-wasip2/release/
	$(CARGO) build --locked --release --target $(WASM_TARGET) $(WASM_PACKAGES)

release-build: plugins ## Build the release binary for TARGET (a target pinned in mise.toml) with the plugins bundled
	FERROLINT_PLUGINS=$(CURDIR)/target/$(WASM_TARGET)/release $(CARGO) build --locked --release --target $(TARGET) -p ferrolint

wheel: release-build ## Build the PyPI wheel for TARGET into target/wheels/, with the plugins bundled
	rm -rf target/wheels
	FERROLINT_PLUGINS=$(CURDIR)/target/$(WASM_TARGET)/release $(MATURIN) build --release --locked --target $(TARGET) -m crates/ferrolint/Cargo.toml --out target/wheels
	@# The musl binary is static: tag it manylinux too so pip on glibc accepts it.
	@case $(TARGET) in *-linux-musl) arch=$(firstword $(subst -, ,$(TARGET))); \
		$(WHEEL) tags --platform-tag manylinux_2_17_$$arch.musllinux_1_2_$$arch --remove target/wheels/*.whl;; esac

smoke: ## Smoke-test the binary BIN: version, help, config validation, exit codes and start-up time
	tools/smoke.sh $(BIN)

bench: ## Run the release benchmarks (AC-26, AC-27): a million lines, ten plugins, against Pylint
	$(CARGO) test --locked --release --test acceptance -- --ignored --nocapture --test-threads=1 ac_26 ac_27

sample-fixtures: ## Record Pylint's W0611 findings for each sample plugin fixture in its .txt sibling
	@for file in $$(find $(SAMPLE_FIXTURES) -name '*.py' | sort); do \
		{ $(PYLINT_W0611) "$$file" || test $$? -lt 32; } | { grep ':W0611:' || true; } | sort -t: -k1,1n -k2,2n \
			> "$${file%.py}.txt.new" && mv "$${file%.py}.txt.new" "$${file%.py}.txt"; \
	done

pylint-compare: ## Compare ferrolint's findings with Pylint's on CODE, reusing PYLINT_OUT when it exists — accepts CODE, BIN (a bundled binary), PYLINT, PYLINT_OUT, IGNORE, PY_VERSION
	BIN="$(BIN)" CODE="$(CODE)" PYLINT="$(PYLINT)" PYLINT_OUT="$(PYLINT_OUT)" IGNORE="$(IGNORE)" PY_VERSION="$(PY_VERSION)" tools/pylint-compare.sh

release-tag: ## Tag the base branch as v<package.version> and push the tag (minor and major bumps)
	@version=$$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1); base=$(BASE:origin/%=%); tag=v$$version; \
	test "$$version" && test "$$(git rev-parse --abbrev-ref HEAD)" = "$$base" && test -z "$$(git status --porcelain)" || { echo "release-tag: run it from a clean $$base, which must have a version in Cargo.toml"; exit 1; }; \
	git fetch --quiet origin && test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/$$base)" || { echo "release-tag: $$base is not at origin/$$base"; exit 1; }; \
	git rev-parse --quiet --verify "refs/tags/$$tag" >/dev/null && { echo "release-tag: $$tag already exists"; exit 1; }; \
	git tag -s "$$tag" -m "$$tag" && git push origin "$$tag"

.PHONY: install-rust clippy fmt fmt-check deny cargo-test build plugins release-build wheel smoke bench sample-fixtures pylint-compare release-tag
