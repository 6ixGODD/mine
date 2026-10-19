# ADR-0035: Local equivalent verification and publication for 0.2.0

## Authority and scope

The owner explicitly prohibited GitHub CI consumption, requested actual 0.2.0
publication, and delegated technical ADR decisions to the release coordinator.
The native registry binds the approval to this body. This is a one-release
execution-location exception, not a quality waiver or permanent CI removal.
ADR-0021 remains the normal delivery contract; ADR-0034 limits this release to
Windows x86_64 and Linux x86_64. Historical failures and unavailable workflow
runs remain recorded as such.

## Decision

For exactly v0.2.0, an independently reviewed local evidence join replaces the
requirement to execute GitHub-hosted jobs. Every applicable quality, payload,
discovery, phase-contract, actual-candidate and native delivery obligation
remains mandatory. Missing, failed or inapplicable evidence refuses publication.
Do not manufacture successful GitHub needs/results or describe NOT_RUN as PASS.

Finish Plan acceptance and final Design synchronization before closure. Freeze
the accepted source S and the curated stable candidate C, including exact commit,
tree, configuration, toolchain/profile and relevant input identities. Preserve
the existing finite layout/purge authorization and actual source-bound approval
and independent acceptance proof. Product, version, workflow and changelog edits
belong in accepted S, never in a curation exemption.

The local join includes the repository's complete decisive quality suite,
generated platform/Skill parity, four-agent installation and actual MCP discovery,
native phase-contract probes, actual C version/Design/ADR/stable-tree hygiene,
and actual installation/failure tests using the final Windows and Linux release
archives and adjacent checksums. Linux container evidence must identify the
image, native target and exact source/artifact; it is not GitHub-runner evidence.
Reuse is permitted only when exact relevant inputs remain applicable. A debug
binary, older product snapshot or fixture-only check cannot validate final
release bytes. Record commands, exit codes, first failures, input keys and
artifact hashes; independent review verifies the complete join.

## Prevent workflow consumption without changing remote settings

Before S is accepted, the release and changelog workflows retain their ordinary
tag pattern but explicitly exclude this tag in order: `['v*', '!v0.2.0']`.
The final curated C commit includes `[skip ci]` in its message before C is frozen.
The branch push is suppressed by GitHub's documented push-event skip semantics;
the tag push is suppressed by explicit tag filtering, not guessed tag-message
semantics. Do not dispatch workflows, push dev/Plan refs, or change repository
Actions settings, secrets, protections or unrelated workflows.

Before remote mutation, inventory the actual candidate workflows and remote
workflow/event configuration. Confirm that the exact stable push, tag push and
release publication cannot trigger another workflow (including create/release
or workflow_run listeners). Unexpected triggers or required remote checks are
a blocker to resolve explicitly, not permission to bypass protections. The
exception does not claim to stop unrelated scheduled or third-party activity.

## Publication and recovery

After local closure and the verified evidence join, normally push only the
curated stable commit and exact new v0.2.0 tag to the verified canonical
`6ixGODD/mine` repository. No force push or replacement of an existing tag.
Verify remote ref identity before creating a draft release with `--verify-tag`;
do not let the release command synthesize a tag from a moving default branch.

Stage exactly the two declared archives, their two adjacent SHA-256 sidecars
and the combined checksum manifest. Verify names, sizes and downloaded bytes
against the locally approved inventory before publishing the draft as stable.
Release notes disclose local verification, GitHub workflow NOT_RUN and macOS
deferral. Verify public assets, tag identity and actual isolated installation
after publication without upgrading the user's global installation.

On partial upload or network failure, inspect the same draft and exact existing
refs/assets before resuming; do not create duplicate releases, overwrite unknown
assets, move tags or claim completion. Keep closure evidence locally, excluding
ignored journals, ephemeral Plan archives and secrets from uploaded assets.

## Implementation and verification ownership

The active final integrator implements the two trigger exclusions, the current
delivery Design cross-reference and focused contract assertions in its existing
workflow/distribution scope. Immutable Plan bodies are not edited. Independent
review checks that v0.2.0 is excluded, other intended release tags still match,
branch pushes retain the normal workflow policy absent the skip instruction,
and publication refusal tests remain effective. Final closure owns the actual
local join, stable commit message, remote inventory and publication verification.
This ADR does not assert that those operations or tests have already succeeded.

## Official sources opened 2026-10-19

- [GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#onpushbranchestagsbranches-ignoretags-ignore): ordered negative tag patterns exclude a specific tag after a positive pattern.
- [Skipping workflow runs](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs): commit skip instructions apply to push and pull_request, not every event; skipped required checks may remain pending.
- [GitHub CLI release create](https://cli.github.com/manual/gh_release_create): `--verify-tag` refuses a missing remote tag and `--draft` allows asset inspection before publication.
