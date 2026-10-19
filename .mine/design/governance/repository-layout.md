# Versioned Governance Namespace and Release Cutover

## Authority and implementation status

[ADR-0023](../decisions/0023-mine-local-governance-layout.md) was approved by
the repository owner on 2026-10-13, with SHA-256
`6AF843730D6A765EB71B5D11C2EF5E05F260AAFA10C18025EE52869B087BFAFD`.
It supersedes ADR-0006 and ADR-0002's path choices, retaining ownership and
ephemeral Plan retention. The native registry records approval and succession;
historical ADR bodies and their digests are immutable.

This leaf specifies the independently accepted 0.2.0 implementation. It does not
claim that acceptance upgrades installed 0.1.13 or relocates a checkout. During bootstrap,
existing `docs/design/`, `docs/plan/`, configuration and graph v4 remain the
operational source of truth. Older path literals elsewhere in Design describe
that bootstrap; this leaf governs the new layout and its cutover boundary.

The implementation centralizes validated paths in `GovernanceLayout`, creates
the fresh child Git controls, and routes native consumers through that layout.
Terminal staging and exact curation validation are implemented behind the bounded
release composition, not a public migration command. Focused real-Git cutover
tests passed, including retained source receipts through release completion;
interruption and complete final-input/native gates remain separate evidence.
Independent acceptance is complete. The actual source-to-stable cutover, final
candidate verification and publication are separate closure operations; determine
their observed completion from the bound release evidence, not this description.

## Canonical roots and ownership

| Artifact | 0.2.0 location | Lifetime |
|---|---|---|
| Design, ownership marker and progressive indexes | `.mine/design/` | Versioned; retained on stable |
| ADR bodies, registry and generated index | `.mine/design/decisions/` | Versioned; retained on stable |
| Plans, workspace marker, graph and generated graph view | `.mine/plan/` | Versioned on development branches; release-purged |
| Implementation/review reports and retained verification evidence | `.mine/plan/reports/` | Same as Plan workspace |
| Configuration and verification policy | `.mine/config.toml`, `.mine/verification-policy.json` | Versioned; retained on stable |
| Verified Design backups | `.mine/runtime/design-backups/` | Ignored local recovery material |
| Journals, locks and mutable outputs | `.mine/runtime/`, `.mine/locks/`, `.mine/build/` | Ignored; separately owned |
| Linked Plan worktrees | `.mine/worktrees/` | Ignored; lifecycle-managed |

Resolve all repository-relative paths from one validated Root/layout authority.
Configuration, init, stores, Design/ADR/graph validation, CLI/MCP, lifecycle,
receipts, scanner, backup and release consume that authority. Linked worktree
invocations still read and mutate the Root-owned graph, not a local copy.
Do not introduce dual writable roots, permanent `docs/` aliases or a generic
multi-layout compatibility framework. Public user documentation may stay in
`docs/`; it is not MINE-owned merely because of its location.

## Tracking and failure behavior

Tracked `.mine/.gitignore` owns the child ignore rules and explicit governance
exceptions specified in ADR-0023. Design, ADR, Plan, graph, report, config and
policy files must be discoverable and stageable with ordinary Git operations,
without force-add. Exceptions do not themselves stage files. Runtime, build,
lock, worktree and backup data remain ignored. Preserve unrelated user rules.

An excluded `.mine/` parent prevents nested exceptions from being discovered.
Initialization and doctor inspect effective repository/local/global exclusions,
identify the responsible rule, and either reconcile an authorized managed rule
or refuse with actionable guidance. They do not silently alter global user
configuration. Check fresh untracked paths and clean clones, not just already
tracked governance files. Verbose negated matches are not ignored-file proof.

Validate ownership and containment before writes or moves. Foreign markers,
destination collisions and symlink/junction escapes must fail without losing
files. An active legacy workspace is not automatically migrated: report the
old-workspace completion path instead of moving active state or inventing
provenance. No new command names or flags are established by this leaf.

## Byte-stable versioned governance inputs

Plan-body, ADR-body and other declared file SHA-256 values continue to hash the
actual bytes specified by their existing contracts. They are not Git object IDs,
hashes of normalized strings, parsed/reserialized documents or hashes with
CRLF/BOM removed. This clarification supplies the checkout stability needed by
the approved layout/cutover; it does not change digest algorithms or approvals.

For fresh 0.2.0 layout, initialization establishes a versioned, MINE-scoped
`.mine/.gitattributes` alongside the child ignore rules. Make that attributes
file normally trackable, including its explicit child ignore exception, and
retain it on stable. Its managed rules cover MINE-owned Markdown/TOML/JSON
Design/ADR/Plan/report/registry/marker files and exact config/policy paths, plus
the two child Git control files. Use the byte-preserving attributes
`-text -filter -ident -working-tree-encoding` for those known paths, rather than
Git normalization. Native/Skill authors still emit new text as UTF-8/LF. Do not
use the `binary` macro, which would also alter diff/merge behavior.
Do not apply text normalization indiscriminately to binary diagrams, evidence
attachments, runtime/build/worktree directories or user documents. Preserve
unrelated rules and do not rewrite the repository-wide wildcard policy or
global Git configuration. The attributes file is layout setup, not a new
configuration registry or runtime authority.

Setting an attribute does not rewrite an existing working file. In particular,
`text eol=lf` alone would allow an author's CRLF working body to be normalized
in the index without changing the bytes subsequently hashed by proposal.
Byte-preserving Git rules avoid that split without per-file legacy exceptions.
At existing proposal/registration/body-freeze and consumption gates, refuse a
new body whose bytes cannot survive the declared Git round trip; report the
specific path/attribute or line-ending conflict. Do not silently rewrite the
file while computing its digest. The owner/author may correct an unfrozen draft
before proposal or handoff; frozen/approved content remains immutable.
An otherwise valid user-supplied non-LF body is not invalid solely for its line
endings if its exact bytes are preserved; no new encoding/hash schema is added.

Effective attributes, not just the generated file's text, are evidence. Inspect
them through bounded/batched Git observation, including higher-precedence
`info/attributes`, deeper attributes and staged-versus-working rules. Conflicting
foreign rules require an actionable refusal, not edits to those rules. Ensure
the authoritative rules are committed in the baseline before preparing a Plan
checkout. Existing consumers must still compare exact file bytes; Git status,
blob equality or a configured `core.autocrlf` value alone proves no raw digest.

Terminal relocation must preserve an already-frozen non-LF body too. It must
not run bulk renormalization or change an approval digest to make validation
pass. Copy only verified source-manifest bytes and preserve the original SHA;
never infer missing original bytes from a normalized Git blob. The candidate
must stage/commit the managed attributes together with the relocated files,
then verify actual raw blob payloads and fresh checkout bytes under those rules.
This handles old LF/CRLF bodies uniformly, without a new exception registry.
If equality cannot be proved, preserve the source and refuse cutover. Include
the exact attributes/ignore bytes in existing cutover recovery evidence, merge
only their clearly managed sections, and refuse ambiguous foreign conflicts.
Encode path patterns correctly; spaces or Git pattern metacharacters cannot
turn bounded ownership into an accidental wildcard over other files.

The implementation integrates init/tracking and native consumers, byte-preserving
cutover, all-five-Skill authoring/setup rules and complete candidate evidence.
Do not edit handed-off Plan bodies or live legacy
attributes as a bootstrap shortcut. The managed child attributes are generated
and tested in isolated candidate repositories, then included in the authorized
terminal candidate with the other layout setup.

Required real-Git evidence includes `core.autocrlf=true`, `false` and `input`,
fresh Root-authored LF bodies, ordinary stage/commit, linked worktree checkout
and clean clone; compute raw SHA at every relevant boundary and run native
ADR/Plan validation. Include no-attributes and conflicting-attributes negative
controls, a CRLF draft proving that normalization cannot silently change the
frozen input, and frozen CRLF terminal relocation with unchanged bytes/approval.
Cover ignored child attributes and unrelated binary/
user files remaining untouched. Windows-native behavior must be exercised, not
inferred from a fixture forcing autocrlf=false. Tests are NOT_RUN by this Design
clarification; the executor must retain actual results and first failures.

## Release retention and bootstrap boundary

The location changes, not retention. Stable keeps Design/ADR/configuration and
excludes the owned ephemeral Plan workspace and its development history under
the existing curated/squash release policy. Cleanup targets exact owned roots;
never delete `.mine/` as one unit. Backup verification, manifests, hash checks
and recursive release exclusion remain required at their new locations.

For development of 0.2.0 by installed 0.1.13:

1. Keep the live legacy config, Design, graph, frozen Plans and reports at their
   existing locations while the old binary performs lifecycle mutations.
2. Implement and test the new layout in isolated candidate repositories, using
   per-Plan mutable output directories. Do not replace the global binary early.
3. Finish acceptance and other old-binary lifecycle writes before the final
   curated stable-tree cutover. Do not rewrite frozen Plan bodies to adjust paths.
4. At that reviewed cutover, preserve durable Design/ADR file bytes and approved
   body digests while relocating the tree. Update current indexes/configuration
   and governance references; do not rewrite historical decisions. Purge the old
   ephemeral workspace under existing release rules instead of migrating its
   history into stable.
5. Validate the resulting stable candidate with the built new binary and its
   matching Skills. On failure preserve original inputs and evidence, report
   pending closure, and do not publish a partially moved or dual-authority tree.

## Implementation coverage and planning constraints

The terminal source-to-candidate proof follows
[exact curation proof and authorization](../operations/verification-receipts.md#exact-curation-proof-and-authorization-boundary).
The manifest is evidence, not permission: the trusted concrete layout validator
recomputes the bounded approved transformation and rejects every unexplained
content or mode change. Generic lifecycle verification does not embed this
repository's Plan/ADR IDs or accept caller-defined transformation scripts.

Implementation must cover native paths and validators, init/doctor ignore
diagnostics, sync/backup, scanners, receipts/lifecycle/release, all five source
Skills, templates/generated/embedded assets, AGENTS generation, fixtures and
public documentation. Path-bearing producers and consumers must agree; a
configuration-only edit or bulk string replacement is insufficient evidence.

Independent active lanes may complete their frozen scope. Do not withdraw,
rewrite or reject an active Plan merely to absorb this requirement. Preserve
existing gate identities, notably plan21's late `plan19 ACCEPTED` gate, unless
an independently evidenced impossibility requires separate replanning. New work
must be assigned before the final 0.2.0 candidate, with disjoint ownership where
possible and explicit integration ownership for shared paths. This Design
change itself registers no Plan and changes no graph edge.

## Acceptance evidence required from implementation

| Boundary | Required observable result |
|---|---|
| Fresh init | Canonical governance roots; unrelated `docs/` bytes unchanged |
| Effective ignore rules | New governance files visible and ordinary add succeeds; mutable/local paths ignored, including clean-clone and parent/local/global exclusion cases |
| Byte stability | Actual frozen SHA values survive Root/index/worktree/clone and terminal cutover under effective Git attributes; no normalization of immutable bodies |
| Root authority | CLI/MCP and linked-worktree invocation resolve the same graph/config/Design/ADR roots |
| Lifecycle and consumers | Plan/report/receipt/scan/backup/release checks use the same resolved layout |
| Stable candidate | Durable Design/config retained; no owned ephemeral Plans, tracked runtime or backups; `.mine/` not purged wholesale |
| Recovery/security | Collision, foreign ownership, path escape and interrupted cutover preserve source data and approved digests |
| Distribution | All five source/generated/embedded Skills and examples match actual binary behavior and help |

These runtime and migration checks are implementation obligations, **NOT_RUN**
by this approval/design-only change. Native Design/readiness validation proves
planning eligibility, not implementation correctness or release readiness.

## Primary sources

Opened 2026-10-12: [Git ignore rules](https://git-scm.com/docs/gitignore)
define precedence, negation, tracked-file behavior and excluded-parent limits;
[Git check-ignore](https://git-scm.com/docs/git-check-ignore) defines verbose
diagnostics and `--no-index`. Use effective-path probes plus real untracked/index
evidence rather than inferring behavior from ignore-file text alone.

Opened 2026-10-14: [Git attributes](https://git-scm.com/docs/gitattributes)
documents LF conversion through `text`/`eol`, `-text` disabling EOL conversion,
attribute precedence and additional filter/ident/encoding transforms. The local
choice is scoped byte preservation plus LF authoring, not normalization with
per-file historical exceptions. Real raw-byte round trips are required; neither
a local autocrlf override nor equality of Git object IDs replaces file SHA checks.
