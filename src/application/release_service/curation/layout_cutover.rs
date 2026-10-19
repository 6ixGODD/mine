//! Concrete, source-authorized MINE 0.2 projection. Repository-specific approval
//! identities live here, never in generic lifecycle/curation evidence decoding.

use super::layout_metadata as metadata;
use super::*;
use crate::domain::adr::{AdrRegistry, AdrStatus, sha256_hex};
use crate::domain::path::is_within;
use crate::infrastructure::governance_git;

const REPOSITORY: &str = "9c672a03-bab3-566e-9588-39cb5e6445c3";
const APPROVING_ADR: &str = "ADR-0023";
const APPROVED_BODY: &str = "6AF843730D6A765EB71B5D11C2EF5E05F260AAFA10C18025EE52869B087BFAFD";
// mine-release-allow-plan-reference: concrete source-bound self-host cutover authorization, not transient provenance
const IMPLEMENTATION: &str = "plan24";
const CODE: &[&str] = &[
    "src/application/release_service/curation/layout_cutover.rs",
    "src/application/release_service/curation/layout_metadata.rs",
    "src/application/release_service/curation/layout_staging.rs",
    "scripts/stage-layout-cutover.rs",
];
const MAX_DESIGN_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct LayoutCutover;

pub(super) struct ProjectedFile {
    pub mode: u32,
    pub bytes: Vec<u8>,
}

/// This is recomputed from S. It is neither deserializable nor caller-created.
pub(super) struct Projection {
    pub changes: BTreeMap<RepoPath, Option<ProjectedFile>>,
    pub authorizations: Vec<FileIdentity>,
    pub source_files: Vec<FileIdentity>,
}

fn refused(reason: &'static str) -> OperationError {
    OperationError::Refused(reason)
}
fn path(value: &str) -> OperationResult<RepoPath> {
    Ok(RepoPath::new(value)?)
}
fn blob(observed: &CurationObservation, path: &RepoPath) -> OperationResult<Vec<u8>> {
    Ok(observed.source_blobs(std::slice::from_ref(path))?.remove(0))
}
fn identity(path: RepoPath, bytes: &[u8]) -> FileIdentity {
    FileIdentity {
        path,
        bytes: bytes.len() as u64,
        sha256: digest(bytes),
    }
}

/// Approval, independent review and integrated implementation all bind S.
/// A candidate manifest/status flag cannot supply any of these facts.
fn authorization(
    observed: &CurationObservation,
    source: &SourceContract,
    registry: &AdrRegistry,
) -> OperationResult<Vec<FileIdentity>> {
    if source.config.repository_id != REPOSITORY || source.config.mine_code_version != "0.2.0" {
        return Err(refused(
            "layout cutover requires the accepted MINE 0.2 source identity/version",
        ));
    }
    let entry = registry
        .get(APPROVING_ADR)
        .filter(|entry| entry.status == AdrStatus::APPROVED)
        .ok_or(refused("source layout ADR is not approved"))?;
    let body_path = path(&entry.path)?;
    let body = blob(observed, &body_path)?;
    if entry.content_sha256 != APPROVED_BODY
        || sha256_hex(&body) != entry.content_sha256
        || entry.approved_by.is_empty()
        || entry.approved_at.is_empty()
    {
        return Err(refused("source layout approval/body digest differs"));
    }
    let implementation = source
        .workspace
        .get(IMPLEMENTATION)
        .filter(|node| node.status == PlanStatus::Accepted)
        .ok_or(refused(
            "source layout implementation has no independent acceptance",
        ))?;
    if !implementation.design_references.contains(&entry.path)
        || implementation.implementation_report.is_empty()
        || implementation.review_report.is_empty()
        || implementation.implementation_commits.is_empty()
        || !["src/application", "src/domain", "tests", "scripts"]
            .iter()
            .all(|required| {
                implementation
                    .exclusive_write_paths
                    .iter()
                    .any(|owned| is_within(owned, required))
            })
    {
        return Err(refused(
            "accepted source implementation does not cover this transformation",
        ));
    }
    // source_contract already verified exact terminal reports/verdicts, source
    // ancestry and accepted integration. Additionally bind this concrete code
    // to the submitted implementation, not later unreviewed product changes.
    let tip = CommitId::new(
        implementation
            .implementation_commits
            .last()
            .ok_or(OperationError::Journal)?
            .clone(),
    )?;
    let requests = CODE
        .iter()
        .map(|name| format!("{}:{name}\n", tip.as_str()))
        .collect::<String>();
    let mut git = checkpoint_git(&observed.root, None)?;
    git.validate_commits(&[tip.as_str().into()])?;
    let response = git.run(
        &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
        requests.as_bytes(),
    )?;
    let response = std::str::from_utf8(&response).map_err(|_| OperationError::Journal)?;
    let mut rows = response.split_terminator('\n');
    let mut authorizations = vec![identity(body_path, &body)];
    for name in CODE {
        let name = path(name)?;
        let expected = observed
            .source_entry(&name)
            .filter(|entry| entry.regular())
            .ok_or(refused(
                "source lacks the accepted concrete layout implementation",
            ))?;
        if rows.next() != Some(format!("{} blob", expected.object.as_str()).as_str()) {
            return Err(refused(
                "layout implementation differs from its accepted source commit",
            ));
        }
        authorizations.push(identity(name.clone(), &blob(observed, &name)?));
    }
    if !response.ends_with('\n') || rows.next().is_some() {
        return Err(OperationError::Journal);
    }
    Ok(authorizations)
}

pub(super) fn projection(observed: &CurationObservation) -> OperationResult<Projection> {
    let source = source_contract(observed)?;
    let old = source.config.layout()?;
    if source.config.design.root != "docs/design/index.md"
        || source.config.design.marker != "docs/design/.mine-design.toml"
        || source.config.plan.root != "docs/plan"
        || source.config.graph.source != "docs/plan/execution-graph.toml"
        || source.config.graph.rendered != "docs/plan/execution-graph.md"
        || source.config.adr.registry != "docs/design/decisions/adr-registry.toml"
        || source.config.adr.rendered_index != "docs/design/decisions/index.md"
    {
        return Err(refused(
            "this concrete cutover does not authorize another source layout",
        ));
    }
    for name in observed.before.entries.keys() {
        let name = std::str::from_utf8(name).map_err(|_| OperationError::Journal)?;
        if [".mine/design", ".mine/plan"]
            .iter()
            .any(|target| is_within(target, name))
        {
            return Err(refused("source already occupies a cutover destination"));
        }
        if [
            ".mine/runtime",
            ".mine/locks",
            ".mine/build",
            ".mine/worktrees",
        ]
        .iter()
        .any(|local| is_within(local, name))
            || name
                .split('/')
                .any(|segment| segment.starts_with("design-backup-"))
        {
            return Err(refused("source contains tracked local/runtime/backup data"));
        }
    }
    let registry_bytes = blob(observed, old.adr_registry())?;
    let registry: AdrRegistry =
        toml::from_str(std::str::from_utf8(&registry_bytes).map_err(|_| OperationError::Journal)?)
            .map_err(|_| OperationError::Journal)?;
    registry.validate_structure()?;
    let mut authorizations = source.authorizations.clone();
    authorizations.extend(authorization(observed, &source, &registry)?);
    authorizations.push(identity(old.adr_registry().clone(), &registry_bytes));
    let design_paths = observed
        .before
        .entries
        .iter()
        .filter(|(name, entry)| {
            !entry.directory()
                && std::str::from_utf8(name)
                    .is_ok_and(|name| is_within(old.design_dir().as_str(), name))
        })
        .map(|(name, entry)| {
            if !entry.regular() || entry.mode != 0o100644 {
                return Err(refused("unsupported Design file type/mode"));
            }
            path(std::str::from_utf8(name).map_err(|_| OperationError::Journal)?)
        })
        .collect::<OperationResult<Vec<_>>>()?;
    let total = design_paths.iter().try_fold(0u64, |total, path| {
        total
            .checked_add(
                observed
                    .source_entry(path)
                    .and_then(|entry| entry.bytes)
                    .ok_or(OperationError::Journal)?,
            )
            .ok_or(OperationError::Journal)
    })?;
    if design_paths.len() > 8192 || total > MAX_DESIGN_BYTES {
        return Err(refused("source Design exceeds bounded cutover inventory"));
    }
    let bodies = observed.source_blobs(&design_paths)?;
    let mut changes = BTreeMap::new();
    let mut source_files = Vec::new();
    for (old_path, body) in design_paths.iter().zip(bodies) {
        source_files.push(identity(old_path.clone(), &body));
        changes.insert(old_path.clone(), None);
        changes.insert(
            path(&metadata::relocate(
                old_path.as_str(),
                old.design_dir().as_str(),
            )?)?,
            Some(ProjectedFile {
                mode: 0o100644,
                bytes: body,
            }),
        );
    }
    for entry in &registry.entries {
        if !is_within(old.decisions_dir().as_str(), &entry.path) {
            return Err(refused("source ADR body is outside owned decisions"));
        }
        let target = path(&metadata::relocate(&entry.path, old.design_dir().as_str())?)?;
        let body = changes
            .get(&target)
            .and_then(Option::as_ref)
            .ok_or(refused("source registry body is absent from owned Design"))?;
        if !entry.content_sha256.is_empty() && sha256_hex(&body.bytes) != entry.content_sha256 {
            return Err(refused(
                "source frozen ADR bytes differ from their original digest",
            ));
        }
    }
    for (name, entry) in &observed.before.entries {
        if entry.directory() {
            continue;
        }
        let name = std::str::from_utf8(name).map_err(|_| OperationError::Journal)?;
        if is_within(old.plan_dir().as_str(), name) {
            if !entry.regular() {
                return Err(refused("unsupported ephemeral Plan entry type"));
            }
            changes.insert(path(name)?, None);
        }
    }
    let config_path = path(".mine/config.toml")?;
    let (target, config_bytes) =
        metadata::configuration(&blob(observed, &config_path)?, &source.config)?;
    changes.insert(
        config_path,
        Some(ProjectedFile {
            mode: 0o100644,
            bytes: config_bytes,
        }),
    );
    let (registry, relocated_registry) =
        metadata::registry(&registry_bytes, &registry, old.design_dir().as_str())?;
    changes.insert(
        path(&target.adr.registry)?,
        Some(ProjectedFile {
            mode: 0o100644,
            bytes: relocated_registry,
        }),
    );
    let index = crate::infrastructure::adr_store::render_index(
        &registry,
        target.layout()?.decisions_dir().as_str(),
    )?;
    changes.insert(
        path(&target.adr.rendered_index)?,
        Some(ProjectedFile {
            mode: 0o100644,
            bytes: index.into_bytes(),
        }),
    );
    for (name, rules) in [
        (".mine/.gitignore", governance_git::IGNORE_RULES),
        (".mine/.gitattributes", governance_git::ATTRIBUTE_RULES),
    ] {
        let name = path(name)?;
        let bytes = if observed.source_entry(&name).is_some() {
            blob(observed, &name)?
        } else {
            vec![]
        };
        if observed.source_entry(&name).is_some() {
            authorizations.push(identity(name.clone(), &bytes));
        }
        if observed
            .source_entry(&name)
            .is_some_and(|entry| !entry.regular() || entry.mode != 0o100644)
        {
            return Err(refused("unsupported source Git control mode/type"));
        }
        changes.insert(
            name,
            Some(ProjectedFile {
                mode: 0o100644,
                bytes: governance_git::merge_section(&bytes, rules)?,
            }),
        );
    }
    let policy = path(".mine/verification-policy.json")?;
    let policy_bytes = blob(observed, &policy)?;
    authorizations.push(identity(policy.clone(), &policy_bytes));
    changes.insert(
        policy,
        Some(ProjectedFile {
            mode: 0o100644,
            bytes: metadata::policy(&policy_bytes, old.design_dir().as_str())?,
        }),
    );
    let agents = path("AGENTS.md")?;
    let agents_bytes = blob(observed, &agents)?;
    authorizations.push(identity(agents.clone(), &agents_bytes));
    changes.insert(
        agents.clone(),
        Some(ProjectedFile {
            mode: 0o100644,
            bytes: metadata::agents(&agents_bytes)?,
        }),
    );
    for (path, after) in &changes {
        if let (Some(before), Some(after)) = (observed.source_entry(path), after) {
            if !before.regular() || before.mode != after.mode {
                return Err(refused(
                    "cutover cannot change a source metadata file type/mode",
                ));
            }
        }
    }
    authorizations.sort_by(|a, b| a.path.cmp(&b.path));
    authorizations.dedup_by(|a, b| a == b);
    Ok(Projection {
        changes,
        authorizations,
        source_files,
    })
}

impl CurationValidator for LayoutCutover {
    fn validate(&self, observed: &CurationObservation) -> OperationResult<CheckedCuration> {
        let projected = projection(observed)?;
        let targets = projected
            .changes
            .iter()
            .filter(|(_, after)| after.is_some())
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let actual = observed.candidate_blobs(&targets)?;
        let mut byte_stable = Vec::new();
        for (path, actual) in targets.iter().zip(actual) {
            let expected = projected
                .changes
                .get(path)
                .and_then(Option::as_ref)
                .ok_or(OperationError::Journal)?;
            let entry = observed
                .candidate_entry(path)
                .ok_or(OperationError::Journal)?;
            if entry.mode != expected.mode || actual != expected.bytes {
                return Err(refused(
                    "candidate bytes/mode differ from the source-derived layout projection",
                ));
            }
            byte_stable.push(identity(path.clone(), &expected.bytes));
        }
        let mut delta = Vec::new();
        for (path, after) in projected.changes {
            let before = observed.source_entry(&path).cloned();
            let actual = observed.candidate_entry(&path).cloned();
            if after.is_none() && actual.is_some() {
                return Err(refused("candidate retains an owned legacy path"));
            }
            if before != actual {
                delta.push(TreeDelta {
                    path,
                    before,
                    after: actual,
                });
            }
        }
        let mut checked = observed.checked_expected(&delta, projected.authorizations)?;
        checked.byte_stable_files = byte_stable;
        Ok(checked)
    }
}
