# Changelog

## 0.2.0

### Added

- Responsibility/namespace ownership, generated-write closure and complete held
  ownership snapshots, with compatibility rechecked at execution admission.
- Durable Plan preparation generations, native Root graph checkpoints and
  recoverable same-operation lifecycle/Git effects.
- Input-bound verification capture/record/reuse with explicit invalidation,
  independent/native checks and evidence-preserving cleanup boundaries.
- Architect-authored interfaces and invariants through ordinary Plans, real
  consumers and independent review.

### Changed

- Fresh governance lives in versioned `.mine/design/` and `.mine/plan/`;
  user-owned `docs/` is preserved. Existing live bootstrap governance completes
  under its governing binary before the reviewed terminal release cutover.
- All five Skills, generated plugins and embedded payloads consume the same
  native contracts. Claude Code, Codex, Pi and OpenCode remain supported.
- Canonical repository/download identity is `6ixGODD/mine`; marketplace identity
  remains independently `mine-is-not-everyones`.
- This release delivers Windows x86_64 and Linux x86_64. macOS delivery is
  deferred until the dashboard phase; there is no macOS 0.2.0 archive.

### Delivery policy

For 0.2.0 only, ADR-0035 requires an independently reviewed local evidence join
instead of hosted execution, without waiving any quality/native/artifact gate.
GitHub workflows: NOT_RUN, not PASS. The exact final archives and checksums,
source/candidate identities, isolated installation and public delivery are
verified separately; metadata and synthetic fixtures are not release authority.
This changelog is part of accepted source before tagging, not a post-tag rewrite.
