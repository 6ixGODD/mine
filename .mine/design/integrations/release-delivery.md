# Gated Release and Verifiable Installation Delivery

## Authority and current boundary

Accepted target under [ADR-0021](../decisions/0021-gated-release-and-verifiable-installation.md).
Candidate runtime defaults, fail-closed workflow joins and checksum staging
implement this contract. Final exact-artifact native checks and independent
closure remain mandatory; neither metadata checks nor fetching the new script
URL is an installer smoke. This contract applies to MINE distribution, not
every managed repository. Installed 0.1.13 is not replaced during development.

## Repository and platform identity

Canonical upstream is `6ixGODD/mine`, with stable source branch `master` for this
repository. Derive bootstrap/update/release defaults, user-facing repository URLs
and package repository metadata from that identity. Keep documented account/repo
overrides for forks/tests. Do not rely on old-name redirects or create new aliases.
The marketplace's independent internal name remains `mine-is-not-everyones`
unless its own identity migration is explicitly approved.

| Supported platform | Native target | Published archive |
|---|---|---|
| Linux x86_64 | x86_64-unknown-linux-gnu | mine-linux-x86_64.tar.gz |
| Windows x86_64 | x86_64-pc-windows-msvc | mine-windows-x86_64.zip |

For 0.2.0, macOS ARM64 is owner-deferred until the dashboard phase under
[ADR-0034](../decisions/0034-defer-macos-delivery-until-dashboard-phase.md).
Do not publish a macOS archive or describe its missing native checks as passed.
Existing macOS implementation can remain, but final canonical release metadata,
generated dispatch, documentation and asset-completeness gates must agree on
the two-platform release. Older product snapshots may still describe the prior
matrix; they are not proof of the final candidate inventory.

Each archive has an adjacent `<archive>.sha256` file naming that exact archive.
The native binary name is mine on Unix and mine.exe on Windows. Intel macOS is
unsupported until a real artifact and native smoke are added; fail before attempting
its phantom archive. Build matrix, installer dispatch, README and update selection
consume the same platform definition and are verified for parity.

## Publication join

For exactly v0.2.0, approved [ADR-0035](../decisions/0035-local-equivalent-v020-publication.md)
changes execution location, not any gate: an independently reviewed local join
must retain complete quality, payload/discovery, actual-candidate, phase-contract
and exact Windows/Linux archive/install evidence. GitHub workflows are NOT_RUN,
never fabricated successful needs. Release/changelog tag filters retain `v*`
then exclude only `!v0.2.0`; the final curated C commit includes `[skip ci]`.
Workflow, version and local changelog changes belong in accepted S, not curation.
Final closure inventories actual remote/event configuration, verifies remote refs
and creates a draft with `--verify-tag`; it must verify uploaded/downloaded bytes
before publication and public installation afterward. No global user upgrade.
This bounded exception does not authorize disabling repository Actions or weaken
the normal fail-closed publication contract below for other releases.

Artifact builds may run alongside required quality/preflight/client/bootstrap
checks for the same release candidate. Publication requires all mandatory jobs
and the complete artifact/checksum matrix to succeed. Failed/skipped mandatory
jobs block publication. No `always()`, ignored error or successful archive build
overrides a failed quality gate. The native preflight observes phase-aware baseline
and candidate evidence under [lifecycle receipts](../governance/lifecycle-receipts.md).

Generated Skill payloads and actual MCP schema/discovery must match the implemented
release, not a stale hardcoded tool count. Current Design lists 19 MCP tools;
future adapter changes update schemas, distribution and discovery tests together.
Neither a local preflight nor this ADR authorizes public push/tag/upload/publish.

## Local closure and tag-run evidence are distinct

Release coordination performs the real source-to-candidate preflight, integration
and closure before pushing the release tag, using the installed governing MINE
version and the authorized Skill workflow. During 0.2.0 self-development this
does not authorize replacing installed 0.1.13 with the candidate to mutate the
real workspace. Preserve the actual candidate/source/closure evidence locally
and bind the authorized tag to that exact stable commit/tree. A pending or failed
real closure forbids tag push. Existing explicit owner publication authority is
still required; neither a fixture nor an ordinary tag name supplies it.

CI has two different responsibilities, both mandatory and honestly named:

- Isolated phase-contract probes exercise the candidate binary's native release
  behavior, refusals and read-only diagnostics. Their PASS is behavioral evidence,
  not an observation of this project's real Release operation or permission to
  publish. Never manufacture a successful real operation from those fixtures.
- Tag-candidate checks inspect the actual checked-out tag commit/tree and resolved
  version, validate its real Design/ADR and stable-tree hygiene, and bind all
  required quality, payload, discovery, native archive/checksum results to that
  candidate. No ephemeral Plan graph or local operation journal is expected in
  the stable tree. These checks must not be replaced by fixture results.

The mandatory preflight job may contain both explicitly labelled checks; its
success asserts only what it actually observed. The final publication join
still requires every mandatory job and full artifact/checksum matrix. Local
closure does not excuse failed CI, and green CI does not excuse missing local
closure. Non-tag runs exercise the applicable candidate and behavioral checks
without publishing; stable-only tree checks are explicit stage conditions, not
swallowed errors. Do not expect standalone release diagnostics to return
`can_release=true` when their contract explicitly performs no current admission.

Do not upload ignored release journals, source Plan archives or local verification
bundles to recreate a live workspace on a runner. No new secret, remote journal
transport, attestation schema or CI approval service is introduced. This split
relies on the existing authorized release operator controlling tag creation; it
does not claim CI independently proves an unprovided local closure or protects
against arbitrary repository administrators pushing unauthorized tags.

## Resolution and verified staging

Resolve latest to one exact release tag first, then pin both archive/checksum to
that identity; never fetch independently moving latest artifacts. Use documented
GitHub release links/metadata and bounded HTTPS requests. Validate tag/asset identity.
Explicit version pinning remains supported. Distinguish no published release,
unsupported platform, rate limit, authorization, network/timeout and malformed
metadata; preserve safe HTTP/reset diagnostics without tokens. Retries are bounded
and never turn rate limiting into a missing-tag diagnosis.

Download to owned staging, verify archive name and SHA-256 before extraction, and
validate extraction paths remain inside staging. Reject traversal, escaping links,
wrong platform/version and ambiguous binary candidates. Stage only the expected
native executable and validate its version/identity before touching an existing
installation. Checksums from the same release provide transfer integrity, not
independent publisher authenticity; no new signing/key service is introduced.

Replace only the known installation path with a recoverable platform-safe sequence:
preserve the old executable, install/verify the new one, then retire the old owned
backup after success. If the platform prevents safe replacement of a running
binary, return pending/actionable recovery rather than false success. Interruption
must preserve either the usable old binary or a proven recoverable swap record.
Cleanup resolves exact owned staging paths; never recursively deletes a broad root.

Bootstrap and self-update share this identity/checksum/staging contract. Agent
refresh uses the verified new embedded payload and existing configuration-backup
rules. A refresh failure is reported separately; it cannot be disguised as a
complete binary-and-Skill update or discard an unrelated Agent configuration.

## Real staged smoke and failure tests

On native supported runners, execute the actual repository bootstrap scripts
against candidate archives/checksums before public Release. Inject an isolated
binary/config/HOME root so no real user installation is touched. Local mock
transport may supply known metadata/assets for deterministic failure cases;
successful script smoke must still perform real extraction/checksum/binary execution.

Cover latest/pinned, HTTP classes, wrong/missing checksum, truncated archive,
unsafe paths, missing platform, clean first install, preserved prior installation,
interrupted swap and self-update/refresh outcomes. Linux container smoke is useful
but does not replace native Windows evidence or justify a macOS pass. macOS
delivery is deferred under ADR-0034. After authorized publication, a public
latest-download probe is separate delivery evidence, not substituted for preflight.

## Verified sources

Opened 2026-10-09: [GitHub jobs.needs](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds)
defines dependency gates; [GitHub rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)
defines HTTP limit evidence; [GitHub release links](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases)
defines latest/version distribution entry points. Details remain distinct from
local checksum authenticity and platform replacement safety.
