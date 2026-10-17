CARGO := cargo
CARGO_DENY_VERSION := 0.20.2
# The crates that build for the plugin target: every workspace member but
# the host-only linter and its core types.
WASM_TARGET := wasm32-wasip2
WASM_PACKAGES := --workspace --exclude ferrolint --exclude ferrolint-core
# The Pylint the sample plugin's fixtures are recorded against. Pinned: a
# checker's verdict changes between versions.
PYLINT ?= uvx pylint@4.1.3
SAMPLE_FIXTURES := plugins/unused-import/fixtures

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

sample-fixtures: ## Record Pylint's W0611 findings for each sample plugin fixture in its .txt sibling
	@for file in $$(find $(SAMPLE_FIXTURES) -name '*.py' | sort); do \
		{ $(PYLINT) --disable=all --enable=unused-import,unused-variable --score=n --persistent=n \
			--msg-template '{line}:{column}:{msg_id}:{symbol}:{msg}' "$$file" || test $$? -lt 32; } \
			| { grep ':W0611:' || true; } | sort -t: -k1,1n -k2,2n > "$${file%.py}.txt.new"; \
		mv "$${file%.py}.txt.new" "$${file%.py}.txt"; \
	done

release-tag: ## Tag the base branch as v<package.version> and push the tag (minor and major bumps)
	@version=$$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1); base=$(BASE:origin/%=%); tag=v$$version; \
	test "$$version" && test "$$(git rev-parse --abbrev-ref HEAD)" = "$$base" && test -z "$$(git status --porcelain)" || { echo "release-tag: run it from a clean $$base, which must have a version in Cargo.toml"; exit 1; }; \
	git fetch --quiet origin && test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/$$base)" || { echo "release-tag: $$base is not at origin/$$base"; exit 1; }; \
	git rev-parse --quiet --verify "refs/tags/$$tag" >/dev/null && { echo "release-tag: $$tag already exists"; exit 1; }; \
	git tag -a "$$tag" -m "$$tag" && git push origin "$$tag"

.PHONY: install-rust clippy fmt fmt-check deny cargo-test build plugins sample-fixtures release-tag
