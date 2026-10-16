SHELL := /bin/bash
.SHELLFLAGS := -euo pipefail -c

NPM := pnpm

# Files to scope quality targets to. Accepts paths or globs.
# Usage: make lint FILES="src/foo.py src/bar.py"
FILES ?=

# The commit range commits-check re-validates in CI (a local hook can be
# skipped with --no-verify). Override CONVENTIONAL_GIT to run a
# project-pinned copy instead of an ephemeral uvx fetch — see
# docs/conventions/commits-check.md.
BASE ?= origin/main
CONVENTIONAL_GIT ?= uvx conventional-git@latest
# Pinned, not @latest: a linter's verdict changes between versions. See
# docs/toolchain/okflint.md and docs/documentation/retag-notes.md.
OKFLINT ?= uvx okflint@0.4.1
TYPESAFE ?= uvx --from typesafe-unofficial-cli@1.1.0 typesafe
DOCS_RETAG := uv run --script tools/docs-retag.py
BRANCH ?= $(or $(GITHUB_HEAD_REF),$(shell git branch --show-current))

.DEFAULT_GOAL := help

# Extension points. Each mk/*.mk file appends its own targets to these
# variables; -include mk/*.mk below picks up every one.
LANG_INSTALL_TARGETS     :=
LANG_CHECK_TARGETS       :=
LANG_FIX_TARGETS         :=
LANG_FIX_UNSAFE_TARGETS  :=
LANG_TEST_TARGETS        :=

-include mk/*.mk

##@ Help

help: ## Show this help message
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) }' $(MAKEFILE_LIST)

##@ Setup

install-tools: ## Install the toolchain pinned in mise.toml
	mise install

install-node: ## Install Node tooling (markdownlint-cli2, cspell) via pnpm
	$(NPM) install

setup-hooks: ## Install the pre-commit git hook
	pre-commit install

install: install-tools install-node $(LANG_INSTALL_TARGETS) setup-hooks ## Install everything

.PHONY: help install-tools install-node setup-hooks install

##@ Read-only checks (never mutate — the CI gate)

makefile-lint: ## Lint the Makefile with checkmake
	checkmake Makefile

md-lint: ## Lint Markdown files with markdownlint-cli2 — accepts FILES="..."
	@if ! command -v pnpm &>/dev/null; then \
		echo "pnpm is not available. Run: make install-node"; \
	else \
		pnpm exec markdownlint-cli2 $(if $(FILES),$(FILES),'**/*.md'); \
	fi

spell: ## Spell-check files with cspell — accepts FILES="..."
	@if ! command -v pnpm &>/dev/null; then \
		echo "pnpm is not available. Run: make install-node"; \
	else \
		pnpm exec cspell --no-progress --no-summary $(if $(FILES),$(FILES),'**'); \
	fi

commits-check: ## Validate the commit range and branch name against Conventional Commits/Branch — see docs/conventions/commits-check.md
	@git log --no-merges --format='%B%x00' $(BASE)..HEAD | while IFS= read -r -d '' message; do \
		message="$${message#$$'\n'}"; \
		echo "$$message" | $(CONVENTIONAL_GIT) check commit || exit 1; \
	done
	@$(CONVENTIONAL_GIT) check branch --name "$(BRANCH)"

help-check: ## Fail if a Makefile target lacks its ## help line — see docs/conventions/help-check.md
	@missing=$$(grep -HnE '^[a-zA-Z][a-zA-Z0-9_-]*[[:space:]]*:([^=]|$$)' Makefile mk/*.mk 2>/dev/null | grep -v '##' || true); \
	test -z "$$missing" || { echo "target without a ## help line:" >&2; echo "$$missing" >&2; exit 1; }

claude-md-check: ## Fail if a CLAUDE.md exists, since AGENTS.md is the only agent file — see docs/conventions/claude-md-check.md
	@found=$$(git ls-files --cached --others --exclude-standard | grep -E '(^|/)CLAUDE\.md$$' || true); \
	test -z "$$found" || { echo "CLAUDE.md found; fold it into AGENTS.md and delete it:" >&2; echo "$$found" >&2; exit 1; }

docs-lint: ## Validate docs/ as an OKF bundle with okflint — see docs/toolchain/okflint.md
	@$(OKFLINT) validate

check: makefile-lint md-lint spell docs-lint commits-check help-check claude-md-check $(LANG_CHECK_TARGETS) ## Run the full read-only validation gate

.PHONY: makefile-lint md-lint spell docs-lint commits-check help-check claude-md-check check

##@ Writable fixes (mutate files in place)

md-fix: ## Auto-fix Markdown files with markdownlint-cli2 — accepts FILES="..."
	@if ! command -v pnpm &>/dev/null; then \
		echo "pnpm is not available. Run: make install-node"; \
	else \
		pnpm exec markdownlint-cli2 --fix $(if $(FILES),$(FILES),'**/*.md'); \
	fi

# Only invoke md-fix with Markdown files — prevents `fix`/`fix-unsafe` from
# forwarding a non-Markdown FILES scope (e.g. FILES="src/main.py") into
# markdownlint-cli2. Not a public target; used internally by fix/fix-unsafe.
_md-fix-scoped:
	@if [ -z "$(FILES)" ]; then $(MAKE) md-fix; else \
		MD_FILES=$$(echo "$(FILES)" | tr ' ' '\n' | grep '\.md$$' | tr '\n' ' ' || true); \
		if [ -n "$$MD_FILES" ]; then $(MAKE) md-fix FILES="$$MD_FILES"; fi; \
	fi

fix: _md-fix-scoped $(LANG_FIX_TARGETS) ## Apply all safe auto-fixes

fix-unsafe: _md-fix-scoped $(LANG_FIX_UNSAFE_TARGETS) ## Apply all auto-fixes, including unsafe ones

.PHONY: md-fix _md-fix-scoped fix fix-unsafe

##@ Test

test: $(LANG_TEST_TARGETS) ## Run the test suite

.PHONY: test

##@ Docs tagging (optional, billed — not part of check)

docs-retag: ## Ask TypeSafe which tags each docs/ note carries, then show a dry run — one billed request per note
	@$(DOCS_RETAG) build
	@$(TYPESAFE) -o jsonl ask --states-file .cache/docs-retag/states.jsonl --questions-file .cache/docs-retag/questions.json --concurrency 4 > .cache/docs-retag/answers.jsonl
	@$(DOCS_RETAG) apply

docs-retag-apply: ## Write the tags from the last docs-retag run into the notes, then validate docs/
	@$(DOCS_RETAG) apply --write
	@$(MAKE) docs-lint

.PHONY: docs-retag docs-retag-apply
