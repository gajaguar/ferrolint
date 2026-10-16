---
type: rule
title: Acceptance criteria - suppressions
description: Given/When/Then criteria for the three inline suppression forms, mapped to FR-05.
tags: [requirements, testing]
status: stable
---

# Acceptance criteria: suppressions

## AC-11: ferrolint-disable comments suppress their codes (FR-05)

* Given a fixture line producing one finding, whose comment is
  `# ferrolint: disable=CODE` for that finding's code,
* then that finding is absent from a run's report, and a second code
  reported on the same line is suppressed only if its own code is in the
  comment's list.

## AC-12: pylint-disable comments suppress their codes (FR-05)

* Given the same fixture line, whose comment is
  `# pylint: disable=CODE` written with the code in Pylint msg-id, symbol
  or plugin spelling,
* then the run suppresses the finding, identically to the spelling
  of AC-11 for the same code.

## AC-13: noqa comments suppress their codes (FR-05)

* Given the same fixture line, whose comment is `# noqa: CODE`, and a
  variant with two comma-separated codes,
* then the run suppresses exactly the findings whose codes the payload
  lists, and a suppression comment is itself inert for codes no finding
  carries.

*Given* one fixture with one finding per form means all three forms are
exercised by the same assertion body: the finding appears without the
comment and disappears with it, in every output format.
