---
type: reference
title: Tag vocabulary
description: The tags the docs/ notes carry and what each one means, which docs-retag reads as its questions.
tags: [documentation]
status: stable
---

# Tag vocabulary

Tags are cross-cutting: they name what a note is about, across directories.
The directory already names the topic. The vocabulary is open: okflint accepts
any tag, and a note may carry one the table does not list, which
`make docs-retag` never touches. A tag in the table is one `make docs-retag`
asks about, so add a row for each new tag worth assigning that way.

A tag is lowercase and one word. Join words with a hyphen only when the
concept's own name has several words (a product, a tool or an established
term), and do not prefix a subtopic with its parent: `hooks`, not `git-hooks`.

| Tag             | A note carries it when it…                                 | A note does not when it…                                       |
| :-------------- | :--------------------------------------------------------- | :------------------------------------------------------------- |
| `documentation` | is about how to write, structure or validate documentation | is about the product or the code rather than its documentation |
| `git`           | is about commits, branches, tags, pushes or pull requests  | does not concern git                                           |
| `makefile`      | is about a `make` target or the `Makefile`                 | does not concern the command surface                           |
| `release`       | is about versions, tags that mark a release, or publishing | does not concern releasing                                     |
| `toolchain`     | is about which tool is installed, by whom and why          | does not concern installing tools                              |
| `versioning`    | is about SemVer and when to bump a version                 | does not concern version numbers                               |
