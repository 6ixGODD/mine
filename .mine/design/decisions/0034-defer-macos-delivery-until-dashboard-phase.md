# ADR-0034: Defer macOS delivery until the dashboard phase

## Authority

The native registry records status and approved digest. On 2026-10-19 the owner
explicitly directed that macOS be left out of 0.2.0 and revisited with the
dashboard. This records that scope decision; it is not a claim that missing
macOS verification passed. ADR-0021's integrity and fail-closed gates remain in
force for platforms included in this release.

## Decision

0.2.0 delivers Windows x86_64 and Linux x86_64. macOS ARM64 delivery and its
native acceptance evidence are DEFERRED until the dashboard phase; Intel macOS
remains unsupported. No macOS archive is published or advertised as supported
in 0.2.0. Preserve reusable macOS implementation and historical evidence rather
than undertaking an unrelated platform-code removal. Re-enablement later needs
real macOS evidence, coherent platform metadata and independent review.

The final canonical `distribution/platforms.json` contains the two in-scope
platforms. Existing generation synchronizes bootstrap and Rust platform tables;
workflow matrices and complete-asset/checksum validation derive from that same
source, not duplicated platform counts, a success override or a skipped required
job. Documentation and release notes explicitly state the deferral. Mac users
must not be directed to a nonexistent 0.2.0 asset.

The final-candidate integrator owns canonical platform metadata, generated
dispatch and release documentation as part of its existing distribution scope.
The active delivery-harness executor retains its current physical scope: verify
Windows/Linux, make workflow/harness checks consume the canonical matrix, and
record macOS as owner-deferred rather than failed or passed. Its older accepted
product snapshot may still contain the previous three-platform metadata; that
is not the final 0.2.0 release inventory. Do not change the pinned product source
silently to make that snapshot appear to be the final release. Final integration
must regenerate and validate the two-platform product and repeat only checks
invalidated by changed inputs, plus required independent final-candidate checks.

This owner decision overrides the older three-platform acceptance scope in
handed-off Plans without editing their immutable bodies. Record the scope delta
in implementation/review reports. It does not waive Windows/Linux verification,
publication refusal tests, exact artifact/source attribution or independent
acceptance. GitHub CI remains unavailable under the separate owner constraint;
this decision does not authorize CI consumption, paid runners or replacement of
workflow-run evidence with a fictitious success.

## Verification

Prove final manifest/generated dispatch parity, exactly two intended archives
and adjacent checksums, actual Windows/Linux installation evidence, rejection of
missing/extra assets under the declared matrix, and no claimed macOS pass or
phantom download. Record Linux container evidence explicitly when used. Real
workflow-run evidence and other resource limitations remain distinct from the
macOS scope deferral. No implementation test is claimed by this ADR.
