//! Exact Git observations are evidence, not a caller-programmable whitelist.
//! Only trusted application composition selects a curation validator. The
//! ordinary implementation permits the source-owned ephemeral Plan purge;
//! layout-specific transformation rules belong to their concrete consumer.
pub use crate::domain::release_evidence::{CurationEvidence, EntryKind, TreeDelta, TreeEntry};
pub(crate) use crate::domain::validation::{validate_terminal_node, validate_terminal_workspace};
use crate::domain::{
    config::MineConfig,
    design_marker::DesignMarker,
    graph::{CommitId, FileIdentity, PlanWorkspace, RepoPath},
    lifecycle_operation::{OperationError, OperationResult},
    status::PlanStatus,
};
use crate::infrastructure::{
    git_evidence::GitRequest,
    graph_checkpoint::{checkpoint_git, digest},
    toml_store::render_markdown,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 100_000;
const MAX_BLOB_BATCH: usize = 16 * 1024 * 1024;

mod layout_cutover;
mod layout_metadata;
mod layout_staging;
pub use layout_staging::{LayoutStageResult, stage_layout_cutover};

#[cfg(test)]
mod tests;

impl TreeEntry {
    fn regular(&self) -> bool {
        self.kind == EntryKind::Blob
            && matches!(self.mode, 0o100644 | 0o100755)
            && self.bytes.is_some()
    }
    fn directory(&self) -> bool {
        self.kind == EntryKind::Tree && self.mode == 0o040000
    }
}

struct TreeSnapshot {
    tree: CommitId,
    entries: BTreeMap<Vec<u8>, TreeEntry>,
}

/// Read-only native object observations; no JSON/manifest constructor exists.
pub struct CurationObservation {
    root: PathBuf,
    source: CommitId,
    candidate: CommitId,
    before: TreeSnapshot,
    after: TreeSnapshot,
    delta: Vec<TreeDelta>,
}

/// This is deliberately not Deserialize/Default and has no public fields or
/// public constructor. A frozen evidence file is never decoded into this proof.
pub(crate) struct CheckedCuration {
    evidence: CurationEvidence,
    request_identity: Option<std::sync::Arc<()>>,
    // Request-local actual-byte observations. Never decoded from a manifest.
    byte_stable_files: Vec<FileIdentity>,
}
impl CheckedCuration {
    pub(crate) fn evidence(&self) -> &CurationEvidence {
        &self.evidence
    }

    pub(crate) fn verify_checkout(&self, root: &Path) -> OperationResult<()> {
        if !self
            .request_identity
            .as_ref()
            .zip(crate::infrastructure::git_evidence::request_identity().as_ref())
            .is_some_and(|(before, current)| std::sync::Arc::ptr_eq(before, current))
        {
            return Err(OperationError::Refused(
                "checked curation belongs to another observation request",
            ));
        }
        if self.byte_stable_files.is_empty() {
            return Ok(());
        }
        let attributes = self
            .byte_stable_files
            .iter()
            .filter(|file| {
                let name = file.path.as_str();
                name.starts_with(".mine/")
                    && (name.ends_with(".md")
                        || name.ends_with(".toml")
                        || name.ends_with(".json")
                        || name == ".mine/.gitignore"
                        || name == ".mine/.gitattributes")
            })
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        crate::infrastructure::governance_git::consume_bodies(root, &attributes)?;
        for file in &self.byte_stable_files {
            let path = crate::infrastructure::operation_store::regular_path(root, &file.path)?;
            let bytes = crate::infrastructure::operation_store::read_bounded(&path)?;
            if bytes.len() as u64 != file.bytes || digest(&bytes) != file.sha256 {
                return Err(OperationError::Refused(
                    "layout checkout changed raw approved/projected bytes",
                ));
            }
        }
        Ok(())
    }
}

pub(crate) trait CurationValidator {
    fn validate(&self, observed: &CurationObservation) -> OperationResult<CheckedCuration>;
}

impl CurationObservation {
    pub(crate) fn observe(
        root: &Path,
        source: &CommitId,
        candidate: &CommitId,
    ) -> OperationResult<Self> {
        let root = root.canonicalize()?;
        let mut git = checkpoint_git(&root, None)?;
        git.validate_commits(&[source.as_str().into(), candidate.as_str().into()])?;
        let before = snapshot(&mut git, source)?;
        let after = snapshot(&mut git, candidate)?;
        let delta = full_delta(&before.entries, &after.entries)?;
        Ok(Self {
            root,
            source: source.clone(),
            candidate: candidate.clone(),
            before,
            after,
            delta,
        })
    }
    pub fn source_entry(&self, path: &RepoPath) -> Option<&TreeEntry> {
        self.before.entries.get(path.as_str().as_bytes())
    }
    pub fn candidate_entry(&self, path: &RepoPath) -> Option<&TreeEntry> {
        self.after.entries.get(path.as_str().as_bytes())
    }
    pub fn delta(&self) -> &[TreeDelta] {
        &self.delta
    }
    pub fn source_blobs(&self, paths: &[RepoPath]) -> OperationResult<Vec<Vec<u8>>> {
        self.blobs(paths, false)
    }
    pub fn candidate_blobs(&self, paths: &[RepoPath]) -> OperationResult<Vec<Vec<u8>>> {
        self.blobs(paths, true)
    }

    fn blobs(&self, paths: &[RepoPath], candidate: bool) -> OperationResult<Vec<Vec<u8>>> {
        let mut requests = BTreeMap::new();
        let mut order = Vec::new();
        for path in paths {
            let entry = if candidate {
                self.candidate_entry(path)
            } else {
                self.source_entry(path)
            }
            .filter(|entry| entry.regular())
            .ok_or(OperationError::Refused(
                "curation file is absent or not regular",
            ))?;
            let bytes = usize::try_from(entry.bytes.ok_or(OperationError::Journal)?)
                .map_err(|_| OperationError::Journal)?;
            if bytes > MAX_BLOB_BATCH {
                return Err(OperationError::Evidence(
                    "curation blob exceeds bounded observation",
                ));
            }
            requests.insert(entry.object.as_str().to_string(), bytes);
            order.push(entry.object.as_str().to_string());
        }
        let mut observed = BTreeMap::new();
        let mut batch = Vec::new();
        let mut bytes = 0usize;
        for (object, size) in requests {
            if !batch.is_empty() && bytes + size + 128 > MAX_BLOB_BATCH {
                observed.extend(blob_batch(&self.root, &batch)?);
                batch.clear();
                bytes = 0;
            }
            bytes += size + 128;
            batch.push((object, size));
        }
        if !batch.is_empty() {
            observed.extend(blob_batch(&self.root, &batch)?);
        }
        order
            .into_iter()
            .map(|object| {
                observed
                    .get(&object)
                    .cloned()
                    .ok_or(OperationError::Journal)
            })
            .collect()
    }

    /// Only reviewed internal validators call this after deriving permission
    /// and expected bytes/fields/modes from S. External data cannot choose it.
    pub(crate) fn checked_expected(
        &self,
        expected: &[TreeDelta],
        authorizations: Vec<FileIdentity>,
    ) -> OperationResult<CheckedCuration> {
        let paths: BTreeSet<_> = expected.iter().map(|change| &change.path).collect();
        if paths.len() != expected.len()
            || expected != self.delta
            || authorizations.is_empty()
            || authorizations
                .iter()
                .map(|file| &file.path)
                .collect::<BTreeSet<_>>()
                .len()
                != authorizations.len()
            || expected.iter().any(|change| {
                change
                    .before
                    .iter()
                    .chain(&change.after)
                    .any(|entry| !entry.regular())
            })
        {
            return Err(OperationError::Refused(
                "curation delta is duplicate, unsupported or not the exact authorized transformation",
            ));
        }
        Ok(CheckedCuration {
            request_identity: crate::infrastructure::git_evidence::request_identity(),
            byte_stable_files: vec![],
            evidence: CurationEvidence {
                schema_version: 1,
                source_commit: self.source.clone(),
                source_tree: self.before.tree.clone(),
                candidate_commit: self.candidate.clone(),
                candidate_tree: self.after.tree.clone(),
                source_authorizations: authorizations,
                delta: self.delta.clone(),
            },
        })
    }
}

/// Trusted concrete composition, not a caller-selectable validator name.
/// A configuration change is never silently treated as ordinary Plan purge.
pub(super) fn release_proof(observed: &CurationObservation) -> OperationResult<CheckedCuration> {
    let config = RepoPath::new(".mine/config.toml")?;
    let before = observed.source_blobs(std::slice::from_ref(&config))?;
    let after = observed.candidate_blobs(std::slice::from_ref(&config))?;
    if before == after {
        OrdinaryPurge.validate(observed)
    } else {
        layout_cutover::LayoutCutover.validate(observed)
    }
}

pub(super) struct OrdinaryPurge;
impl CurationValidator for OrdinaryPurge {
    fn validate(&self, observed: &CurationObservation) -> OperationResult<CheckedCuration> {
        let authority = source_contract(observed)?;
        let plan_root = &authority.plan_root;
        let candidate_config_bytes = observed
            .candidate_blobs(&[RepoPath::new(".mine/config.toml")?])?
            .remove(0);
        let candidate_config = MineConfig::parse(
            Path::new(".mine/config.toml"),
            std::str::from_utf8(&candidate_config_bytes).map_err(|_| OperationError::Journal)?,
        )?;
        if candidate_config != authority.config {
            return Err(OperationError::Refused(
                "ordinary purge cannot authorize configuration or layout changes",
            ));
        }
        let prefix = format!("{}/", plan_root.as_str());
        let owned = |path: &[u8]| path.starts_with(prefix.as_bytes());
        if observed.after.entries.keys().any(|path| owned(path)) {
            return Err(OperationError::Refused(
                "candidate retains the ephemeral workspace",
            ));
        }
        let expected = observed
            .before
            .entries
            .iter()
            .filter(|(path, entry)| owned(path) && !entry.directory())
            .map(|(path, entry)| {
                Ok(TreeDelta {
                    path: RepoPath::new(
                        std::str::from_utf8(path).map_err(|_| OperationError::Journal)?,
                    )?,
                    before: Some(entry.clone()),
                    after: None,
                })
            })
            .collect::<OperationResult<Vec<_>>>()?;
        if expected.is_empty() {
            return Err(OperationError::Refused(
                "source workspace contains no owned graph files",
            ));
        }
        observed.checked_expected(&expected, authority.authorizations)
    }
}

pub(crate) struct SourceContract {
    pub(crate) config: MineConfig,
    pub(crate) workspace: PlanWorkspace,
    pub(crate) plan_root: RepoPath,
    pub(crate) authorizations: Vec<FileIdentity>,
}

/// Original ownership/terminal source contract only, not release approval.
/// Concrete layout validators additionally verify their S-bound approvals.
pub(crate) fn source_contract(observed: &CurationObservation) -> OperationResult<SourceContract> {
    let config_path = RepoPath::new(".mine/config.toml")?;
    let config_bytes = observed
        .source_blobs(std::slice::from_ref(&config_path))?
        .remove(0);
    let config = MineConfig::parse(
        Path::new(config_path.as_str()),
        std::str::from_utf8(&config_bytes).map_err(|_| OperationError::Journal)?,
    )?;
    uuid::Uuid::parse_str(&config.repository_id)
        .map_err(|_| OperationError::Refused("source repository UUID is invalid"))?;
    let marker_path = RepoPath::new(config.design.marker.clone())?;
    let source_path = RepoPath::new(config.graph.source.clone())?;
    let view_path = RepoPath::new(config.graph.rendered.clone())?;
    let files =
        observed.source_blobs(&[marker_path.clone(), source_path.clone(), view_path.clone()])?;
    let marker = DesignMarker::parse(
        Path::new(marker_path.as_str()),
        std::str::from_utf8(&files[0]).map_err(|_| OperationError::Journal)?,
    )?;
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&files[1]).map_err(|_| OperationError::Journal)?)
            .map_err(|_| OperationError::Journal)?;
    crate::domain::validation::validate_with_worktrees_root(
        &workspace,
        &config.plan.worktrees_root,
    )?;
    let plan_root = RepoPath::new(config.plan.root.clone())?;
    let retained_paths = [
        &config.design.root,
        &config.design.marker,
        &config.adr.registry,
        &config.adr.rendered_index,
    ];
    if marker.schema_version != DesignMarker::SCHEMA_VERSION
        || marker.managed_by != DesignMarker::MANAGED_BY
        || marker.repository_id != config.repository_id
        || workspace.workspace_id.is_empty()
        || !config.plan.ephemeral
        || !config.plan.purge_before_stable_release
        || !workspace.ephemeral_workspace
        || !workspace.purge_before_stable_release
        || workspace.design_root != config.design.root
        || workspace.stable_branch != config.branches.stable
        || workspace.integration_branch != config.branches.integration
        || crate::domain::path::is_within(plan_root.as_str(), config_path.as_str())
        || retained_paths
            .iter()
            .any(|path| crate::domain::path::is_within(plan_root.as_str(), path))
        || !observed
            .source_entry(&RepoPath::new(config.design.root.clone())?)
            .is_some_and(TreeEntry::regular)
        || source_path == view_path
        || !crate::domain::path::is_within(plan_root.as_str(), source_path.as_str())
        || !crate::domain::path::is_within(plan_root.as_str(), view_path.as_str())
        || files[2] != render_markdown(&workspace)?.as_bytes()
    {
        return Err(OperationError::Refused(
            "source does not authorize this owned ephemeral workspace purge",
        ));
    }
    validate_terminal_workspace(&workspace)?;
    let baseline = CommitId::new(workspace.stable_baseline_commit.clone())?;
    let mut git = checkpoint_git(&observed.root, None)?;
    git.validate_commits(&[baseline.as_str().into()])?;
    if !git.is_ancestor(baseline.as_str(), observed.source.as_str())?
        || !git.is_ancestor(baseline.as_str(), observed.candidate.as_str())?
    {
        return Err(OperationError::Refused(
            "curation source/candidate do not share the declared stable baseline",
        ));
    }
    let reviews = source_reviews(observed, &workspace, &plan_root, &mut git)?;

    let identity = |path, body: &[u8]| FileIdentity {
        path,
        bytes: body.len() as u64,
        sha256: digest(body),
    };
    let mut authorizations = vec![
        identity(config_path, &config_bytes),
        identity(marker_path, &files[0]),
        identity(source_path, &files[1]),
        identity(view_path, &files[2]),
    ];
    authorizations.extend(reviews);
    Ok(SourceContract {
        config,
        workspace,
        plan_root,
        authorizations,
    })
}

/// Preserve historical accepted evidence as historical evidence; never invent
/// a modern operation/writer assertion for a legacy node. Native acceptance
/// records retain their additional pinned facts through the source graph.
fn source_reviews(
    observed: &CurationObservation,
    workspace: &PlanWorkspace,
    plan_root: &RepoPath,
    git: &mut GitRequest,
) -> OperationResult<Vec<FileIdentity>> {
    let mut paths = BTreeSet::new();
    let mut verdicts = BTreeMap::new();
    let mut commits = BTreeSet::new();
    for node in &workspace.plans {
        if !matches!(node.status, PlanStatus::Accepted | PlanStatus::Rejected) {
            continue;
        }
        let body = RepoPath::new(node.path.clone())?;
        let stem = body
            .as_str()
            .strip_prefix(&format!("{}/", plan_root.as_str()))
            .and_then(|value| value.strip_suffix(".md"))
            .filter(|value| !value.contains('/'))
            .ok_or(OperationError::Refused("terminal source Plan path differs"))?;
        let report = RepoPath::new(node.review_report.clone())?;
        if report.as_str() != format!("{}/reports/{stem}-review.md", plan_root.as_str())
            || verdicts.insert(report.clone(), node.status).is_some()
        {
            return Err(OperationError::Refused(
                "terminal source review path differs",
            ));
        }
        paths.insert(body.clone());
        paths.insert(report);
        if node.status == PlanStatus::Accepted {
            let implementation = RepoPath::new(node.implementation_report.clone())?;
            if implementation.as_str()
                != format!("{}/reports/{stem}-implementation.md", plan_root.as_str())
                || node.implementation_commits.is_empty()
            {
                return Err(OperationError::Refused(
                    "accepted source implementation evidence missing",
                ));
            }
            paths.insert(implementation);
            for commit in &node.implementation_commits {
                commits.insert(commit.clone());
            }
            if let Some(acceptance) = &node.acceptance {
                let facts = acceptance.fields();
                let review = facts.review.as_ref().ok_or(OperationError::Journal)?;
                commits.insert(review.reviewed_tip.as_str().into());
                commits.insert(
                    facts
                        .integrated_commit
                        .as_ref()
                        .ok_or(OperationError::Journal)?
                        .as_str()
                        .into(),
                );
            }
        }
    }
    let paths = paths.into_iter().collect::<Vec<_>>();
    let size = paths.iter().try_fold(0u64, |total, path| {
        let bytes = observed
            .source_entry(path)
            .and_then(|entry| entry.bytes)
            .ok_or(OperationError::Refused("terminal source evidence absent"))?;
        total.checked_add(bytes).ok_or(OperationError::Journal)
    })?;
    if paths.len() > 8192 || size > 64 * 1024 * 1024 {
        return Err(OperationError::Evidence(
            "terminal source evidence exceeds bounded observation",
        ));
    }
    let bodies = observed.source_blobs(&paths)?;
    let mut identities = Vec::new();
    for (path, body) in paths.into_iter().zip(bodies) {
        if let Some(status) = verdicts.get(&path) {
            crate::application::operation_service::validate_review_verdict(
                &body,
                *status == PlanStatus::Accepted,
            )?;
        }
        identities.push(FileIdentity {
            path,
            bytes: body.len() as u64,
            sha256: digest(&body),
        });
    }
    // All accepted implementation tips must be in S. One set-difference walk,
    // not one merge-base child per Plan. Rejected tips deliberately stay out.
    if !commits.is_empty() {
        let commits = resolve_commit_evidence(git, &commits, observed.source.as_str().len())?;
        let mut input = commits
            .into_iter()
            .map(|commit| format!("{commit}\n"))
            .collect::<String>();
        input.push_str(&format!("^{}\n", observed.source.as_str()));
        if !git
            .run(&["rev-list", "--max-count=1", "--stdin"], input.as_bytes())?
            .is_empty()
        {
            return Err(OperationError::Refused(
                "accepted implementation is not integrated into source",
            ));
        }
    }
    Ok(identities)
}

/// Supported historical graphs retain abbreviated hexadecimal commit evidence.
/// Resolve all of it in one native ordered batch, never rewrite that history or
/// accept symbolic/missing/ambiguous/tag evidence as an immutable commit.
fn resolve_commit_evidence(
    git: &mut GitRequest,
    requested: &BTreeSet<String>,
    width: usize,
) -> OperationResult<BTreeSet<String>> {
    if requested.is_empty() {
        return Ok(BTreeSet::new());
    }
    if requested.iter().any(|value| {
        value.len() < 4
            || value.len() > width
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err(OperationError::Refused(
            "implementation commit evidence is not immutable hexadecimal identity",
        ));
    }
    let input = requested
        .iter()
        .map(|value| format!("{value}\n"))
        .collect::<String>();
    let output = git.run(
        &["cat-file", "--batch-check=%(objectname) %(objecttype)"],
        input.as_bytes(),
    )?;
    decode_commit_evidence(&output, requested, width)
}

fn decode_commit_evidence(
    output: &[u8],
    requested: &BTreeSet<String>,
    width: usize,
) -> OperationResult<BTreeSet<String>> {
    let text = std::str::from_utf8(output).map_err(|_| OperationError::Journal)?;
    let mut lines = text.split_terminator('\n');
    let mut resolved = BTreeSet::new();
    for request in requested {
        let fields = lines
            .next()
            .ok_or(OperationError::Evidence(
                "truncated implementation commit evidence",
            ))?
            .split(' ')
            .collect::<Vec<_>>();
        if fields.len() != 2
            || fields[1] != "commit"
            || fields[0].len() != width
            || !fields[0].starts_with(request)
            || CommitId::new(fields[0]).is_err()
        {
            return Err(OperationError::Refused(
                "missing, ambiguous, foreign or non-commit implementation evidence",
            ));
        }
        resolved.insert(fields[0].into());
    }
    if !text.ends_with('\n') || lines.next().is_some() {
        return Err(OperationError::Evidence(
            "implementation commit response framing",
        ));
    }
    Ok(resolved)
}

/// Diagnostic evidence only. Native release admission additionally binds the
/// repository/workspace/operation, retained source, independent review and gates.
pub fn inspect_purge_candidate(
    root: &Path,
    source: &CommitId,
    candidate: &CommitId,
) -> OperationResult<CurationEvidence> {
    let observed = CurationObservation::observe(root, source, candidate)?;
    Ok(OrdinaryPurge.validate(&observed)?.evidence().clone())
}

/// Read-only diagnostic of the concrete cutover and its actual registered C
/// checkout. Returned evidence is data, not Release admission/publication power.
pub fn inspect_layout_candidate(
    root: &Path,
    source: &CommitId,
    candidate: &CommitId,
    checkout: &RepoPath,
) -> OperationResult<CurationEvidence> {
    let _request = crate::infrastructure::git_evidence::GitRequestScope::join_or_new();
    let root = root.canonicalize()?;
    let target =
        crate::infrastructure::operation_store::regular_path(&root, checkout)?.canonicalize()?;
    let mut git = checkpoint_git(&root, None)?;
    let inventory = git.worktrees()?;
    if inventory
        .first()
        .and_then(|entry| Path::new(&entry.path).canonicalize().ok())
        .as_ref()
        != Some(&root)
        || target == root
        || inventory
            .iter()
            .filter(|entry| {
                Path::new(&entry.path).canonicalize().ok().as_ref() == Some(&target)
                    && entry.head == candidate.as_str()
                    && entry.detached
                    && !entry.locked
                    && !entry.prunable
                    && !entry.bare
            })
            .count()
            != 1
    {
        return Err(OperationError::Refused(
            "layout diagnostic needs actual Root and one exact registered C checkout",
        ));
    }
    let observed = CurationObservation::observe(&root, source, candidate)?;
    let checked = layout_cutover::LayoutCutover.validate(&observed)?;
    let mut target_git = checkpoint_git(&target, None)?;
    let common = git.text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let target_common =
        target_git.text(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    if Path::new(&common).canonicalize()? != Path::new(&target_common).canonicalize()?
        || target_git.text(&["rev-parse", "--verify", "HEAD^{tree}"])?
            != checked.evidence().candidate_tree.as_str()
    {
        return Err(OperationError::Refused(
            "layout checkout repository/tree identity differs",
        ));
    }
    super::retained::verification::prove_checkout_bytes(&target, &mut target_git, candidate)?;
    checked.verify_checkout(&target)?;
    Ok(checked.evidence().clone())
}

fn snapshot(git: &mut GitRequest, commit: &CommitId) -> OperationResult<TreeSnapshot> {
    let tree = CommitId::new(git.text(&[
        "rev-parse",
        "--verify",
        &format!("{}^{{tree}}", commit.as_str()),
    ])?)?;
    let output = git.run(
        &[
            "ls-tree",
            "-r",
            "-t",
            "-l",
            "-z",
            "--full-tree",
            commit.as_str(),
            "--",
        ],
        b"",
    )?;
    let mut entries = BTreeMap::new();
    if output.is_empty() {
        return Ok(TreeSnapshot { tree, entries });
    }
    let framed = output
        .strip_suffix(&[0])
        .ok_or(OperationError::Evidence("curation tree framing"))?;
    for line in framed.split(|byte| *byte == 0) {
        let tab = line
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or(OperationError::Evidence("curation tree framing"))?;
        let fields = std::str::from_utf8(&line[..tab])
            .map_err(|_| OperationError::Journal)?
            .split_ascii_whitespace()
            .collect::<Vec<_>>();
        if fields.len() != 4 || line[tab + 1..].is_empty() {
            return Err(OperationError::Evidence("curation tree fields"));
        }
        let kind = match fields[1] {
            "blob" => EntryKind::Blob,
            "tree" => EntryKind::Tree,
            "commit" => EntryKind::Commit,
            _ => return Err(OperationError::Evidence("curation tree type")),
        };
        let bytes = if kind == EntryKind::Blob {
            Some(fields[3].parse().map_err(|_| OperationError::Journal)?)
        } else {
            if fields[3] != "-" {
                return Err(OperationError::Journal);
            }
            None
        };
        let entry = TreeEntry {
            kind,
            mode: u32::from_str_radix(fields[0], 8).map_err(|_| OperationError::Journal)?,
            object: CommitId::new(fields[2])?,
            bytes,
        };
        if entry.object.as_str().len() != commit.as_str().len()
            || entries.insert(line[tab + 1..].to_vec(), entry).is_some()
            || entries.len() > MAX_ENTRIES
        {
            return Err(OperationError::Evidence(
                "duplicate, oversized or wrong-width curation inventory",
            ));
        }
    }
    for (path, entry) in &entries {
        if entry.directory() {
            let mut prefix = path.clone();
            prefix.push(b'/');
            if !entries
                .range(prefix.clone()..)
                .next()
                .is_some_and(|(next, _)| next.starts_with(&prefix))
            {
                return Err(OperationError::Refused(
                    "empty Git tree entries are unsupported curation inputs",
                ));
            }
        }
    }
    Ok(TreeSnapshot { tree, entries })
}

fn full_delta(
    before: &BTreeMap<Vec<u8>, TreeEntry>,
    after: &BTreeMap<Vec<u8>, TreeEntry>,
) -> OperationResult<Vec<TreeDelta>> {
    let paths = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
    let mut delta = Vec::new();
    for path in paths {
        let old = before.get(path);
        let new = after.get(path);
        // Directory content identities are derived from the complete children.
        // Standalone empty directories were refused above; structural parent
        // creation/removal is not an extra caller-authorized file mutation.
        if old == new
            || old.is_some_and(TreeEntry::directory) && new.is_some_and(TreeEntry::directory)
            || old.is_none() && new.is_some_and(TreeEntry::directory)
            || new.is_none() && old.is_some_and(TreeEntry::directory)
        {
            continue;
        }
        let path = RepoPath::new(
            std::str::from_utf8(path)
                .map_err(|_| OperationError::Refused("changed curation path is not UTF-8"))?,
        )?;
        delta.push(TreeDelta {
            path,
            before: old.cloned(),
            after: new.cloned(),
        });
    }
    Ok(delta)
}

fn blob_batch(
    root: &Path,
    requests: &[(String, usize)],
) -> OperationResult<BTreeMap<String, Vec<u8>>> {
    let input = requests
        .iter()
        .map(|(object, _)| format!("{object}\n"))
        .collect::<String>();
    let mut git = checkpoint_git(root, None)?;
    let output = git.run(&["cat-file", "--batch"], input.as_bytes())?;
    decode_blob_batch(&output, requests)
}

fn decode_blob_batch(
    output: &[u8],
    requests: &[(String, usize)],
) -> OperationResult<BTreeMap<String, Vec<u8>>> {
    let mut remaining = output;
    let mut results = BTreeMap::new();
    for (object, size) in requests {
        let end = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(OperationError::Evidence("curation blob framing"))?;
        let header = std::str::from_utf8(&remaining[..end]).map_err(|_| OperationError::Journal)?;
        if header != format!("{object} blob {size}") {
            return Err(OperationError::Evidence(
                "curation blob identity/type/size differs",
            ));
        }
        remaining = &remaining[end + 1..];
        let body = remaining
            .get(..*size)
            .ok_or(OperationError::Evidence("truncated curation blob"))?;
        if remaining.get(*size) != Some(&b'\n') {
            return Err(OperationError::Evidence("curation blob framing"));
        }
        if results.insert(object.clone(), body.to_vec()).is_some() {
            return Err(OperationError::Evidence("duplicate curation blob response"));
        }
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err(OperationError::Evidence("trailing curation blob data"));
    }
    Ok(results)
}
