# ADR-0021: Gated Release and Verifiable Installation

## Status and scope

Repository-owner review candidate. Covers repository identity, published
artifacts, installer/update safety and actual release gates. README link repair
is separate behavior-preserving maintenance, not implementation of this ADR.

## Context and evidence

The owner renamed the GitHub repository to `6ixGODD/mine`. Local origin,
bootstrap release defaults, `src/setup/release_meta.rs:9` and self-update
defaults still refer to `mine-is-not-everyones`. Marketplace source repository
and marketplace internal name are distinct identities; renaming one does not
silently rename the other.

`.github/workflows/release.yml:283-358` builds artifacts independently; publishing
depends only on release-artifacts, not required quality/preflight/setup/bootstrap
jobs. Existing bootstrap jobs invoke local `mine init`/setup, not the README
installer. `scripts/bootstrap.sh:34-35` masks HTTP failure behind a missing-tag
message. It accepts Intel macOS while the artifact matrix has only macOS ARM64.
The workflow emits SHA-256 files, but bootstrap and self-update do not check them
before installation. A successfully fetched script is not an installation smoke.

## Proposed decision

1. Define `6ixGODD/mine` as canonical repository identity in distribution Design,
   release defaults, bootstrap/update, package metadata and user documentation.
   Preserve independently named marketplace/package identifiers unless their
   own migration is explicitly approved. Remove obsolete internal defaults;
   do not depend on old-repository redirects as the product contract.
2. Publication depends on every required platform quality, preflight and real
   installation-smoke result for the exact release candidate. Artifact builds
   may run concurrently; the publication join fails closed on a failed/skipped
   mandatory job. Never use `always()`/ignored errors to bypass these gates.
3. One supported-platform matrix governs artifact names, README promises,
   installers and self-update. Initial declared support follows existing real
   artifacts: Linux x86_64, macOS ARM64 and Windows x86_64. Intel macOS must
   report unsupported before download unless a separately verified artifact
   and smoke are added; no phantom platform support.
4. Keep explicit version pinning and define latest resolution without conflating
   no release, API rate limit, authorization, network and malformed response.
   Prefer GitHub's documented release endpoints where appropriate; preserve
   status/reset evidence and bounded timeouts/retries. Never suggest user quota
   purchases or missing tags when the observed cause is an HTTP limit.
5. Download the exact platform archive and checksum, validate expected identity
   and SHA-256 before extraction/installation, stage safely, verify the candidate
   executable/version, then replace the owned binary recoverably. Reject unsafe
   archive paths and ambiguous binary locations. Failed validation leaves the
   existing install usable and cleans only owned temporary paths.
6. Checksums from the same release protect transfer integrity, not independent
   publisher authenticity. No signing/key infrastructure is implied. Update
   follows the same verification/replacement contract, not a separate weaker path.
7. CI exercises the actual repository bootstrap scripts on clean supported
   environments with staged candidate archives/checksums before public Release.
   Cover latest/pinned, failed HTTP, mismatch/truncation, missing platform and
   existing-install preservation. Stage tests must not install over the agent's
   real HOME or require publication to prove local installation.

## Alternatives and consequences

- Keeping old URLs via redirects is accidental behavior, not canonical identity.
- Publishing after builds alone repeats the observed release/CI failure gap.
- Testing `mine init` or a local executable does not test download/extraction.
- Advertising Intel macOS without an artifact gives predictable installation
  failure. Adding support is useful but requires explicit build/smoke ownership.

The selected contract closes delivery from approved candidate to usable binary.
It does not authorize push, tag, public upload or modifying users' local installs
during this proposal. Exact staging/replacement algorithms and scripts belong
in detailed Design and bounded implementation Plans after approval.

## Verification obligations

Prove a required quality failure prevents public release even when every archive
build succeeds. Exercise native Windows/macOS and clean Linux installer paths,
checksum failure, archive traversal, nonempty prior installation, interrupted
replacement, unsupported architecture, redirects and each HTTP error class.
Validate documentation/default/matrix parity and distinguish staged smoke from
a post-publication latest-download probe.

## Primary sources

Opened 2026-10-09.

- GitHub, [Workflow syntax: jobs.needs](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idneeds):
  required dependencies control whether downstream jobs can execute.
- GitHub, [REST API rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api):
  distinguish HTTP rate-limit evidence from absence of release metadata.
- GitHub, [Linking to releases](https://docs.github.com/en/repositories/releasing-projects-on-github/linking-to-releases):
  documented latest/version links provide stable distribution entry points.

## Approval question

Approve canonical identity, mandatory publication joins, truthful supported
platforms, verified recoverable installation/update, and real staged installer
smokes before publication?
