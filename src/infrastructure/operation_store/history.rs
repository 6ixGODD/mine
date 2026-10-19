//! A completed release is a derived seal, never a mutable registry/status.
//! Historical validation rehashes retained authority and actual stable history;
//! it does not re-run old checks or require retired S/C/worktrees to exist.
use super::*;
use crate::domain::{
    config::MineConfig,
    design_marker::DesignMarker,
    graph::*,
    lifecycle_operation::*,
    release_evidence::{EntryKind, ReleaseManifest},
    validation,
};
use crate::infrastructure::{
    graph_checkpoint::{WorkspacePredecessor, checkpoint_git, digest},
    strict_json::strict_json,
    toml_store::render_markdown,
};
use std::collections::{BTreeMap, BTreeSet};

fn bundle(id: &OperationId, role: &str, path: &str) -> OperationResult<RepoPath> {
    Ok(RepoPath::new(format!(
        ".mine/runtime/release-evidence/{}/{role}{path}",
        id.as_str()
    ))?)
}
fn exact(root: &Path, file: &FileIdentity) -> OperationResult<Vec<u8>> {
    let body = read_bounded(&regular_path(root, &file.path)?)?;
    if body.len() as u64 != file.bytes || digest(&body) != file.sha256 {
        return Err(OperationError::Refused("retired release authority changed"));
    }
    Ok(body)
}
fn source(root: &Path, manifest: &ReleaseManifest, path: &str) -> OperationResult<Vec<u8>> {
    let retained = manifest
        .files
        .iter()
        .find(|file| file.original.path.as_str() == path)
        .ok_or(OperationError::Evidence(
            "required retired authority missing",
        ))?;
    let mut file = retained.original.clone();
    file.path = bundle(&manifest.operation_id, "source/", path)?;
    exact(root, &file)
}

pub(crate) fn seal(root: &Path, record: &JournalRecord) -> OperationResult<WorkspacePredecessor> {
    record.validate()?;
    let fields = record.operation.fields();
    if fields.stage != OperationStage::Release(ReleaseStage::Complete)
        || fields.outcome != OperationOutcome::Committed
        || fields.plan.is_some()
        || fields.registered_worktree.is_some()
        || fields.review.is_some()
        || fields.cleanup.is_some()
        || fields.checkpoint.is_some()
    {
        return Err(OperationError::Refused(
            "workspace has no genuine completed Release",
        ));
    }
    let identity = fields
        .verification_evidence
        .first()
        .ok_or(OperationError::Journal)?;
    if identity.path != bundle(&fields.operation_id, "", "manifest.json")? {
        return Err(OperationError::Journal);
    }
    let manifest: ReleaseManifest = strict_json(&exact(root, identity)?)?;
    let Some(ClosureBinding::Release {
        baseline,
        candidate,
    }) = &record.closure
    else {
        return Err(OperationError::Journal);
    };
    let integrated = fields
        .integrated_commit
        .as_ref()
        .ok_or(OperationError::Journal)?;
    if manifest.schema_version != 1
        || manifest.curation.schema_version != 1
        || manifest.operation_id != fields.operation_id
        || manifest.repository_id != fields.repository_id
        || manifest.workspace_id != fields.workspace_id
        || manifest.graph_revision != fields.observed_graph_revision
        || manifest.baseline != *baseline
        || manifest.curation.candidate_commit != *candidate
        || manifest.source_commit != manifest.curation.source_commit
        || manifest.selected_plans.is_empty()
        || manifest.files.is_empty()
        || manifest.files.len() > 8192
    {
        return Err(OperationError::Refused(
            "retired Release manifest binding differs",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for retained in &manifest.files {
        if !paths.insert(retained.original.path.clone())
            || retained.source_entry.kind != EntryKind::Blob
            || !matches!(retained.source_entry.mode, 0o100644 | 0o100755)
            || retained.source_entry.bytes.is_none()
        {
            return Err(OperationError::Journal);
        }
        total = total
            .checked_add(retained.original.bytes)
            .ok_or(OperationError::Journal)?;
        if total > 64 * 1024 * 1024 {
            return Err(OperationError::Evidence(
                "retired authority retention limit",
            ));
        }
        let mut copied = retained.original.clone();
        copied.path = bundle(
            &manifest.operation_id,
            "source/",
            retained.original.path.as_str(),
        )?;
        exact(root, &copied)?;
    }
    let config_bytes = source(root, &manifest, ".mine/config.toml")?;
    let config = MineConfig::parse(
        Path::new(".mine/config.toml"),
        std::str::from_utf8(&config_bytes).map_err(|_| OperationError::Journal)?,
    )?;
    let graph_bytes = source(root, &manifest, &config.graph.source)?;
    let workspace: PlanWorkspace =
        toml::from_str(std::str::from_utf8(&graph_bytes).map_err(|_| OperationError::Journal)?)
            .map_err(|_| OperationError::Journal)?;
    validation::validate_with_worktrees_root(&workspace, &config.plan.worktrees_root)?;
    validation::validate_terminal_workspace(&workspace)?;
    let marker_bytes = source(root, &manifest, &config.design.marker)?;
    let marker = DesignMarker::parse(
        Path::new(&config.design.marker),
        std::str::from_utf8(&marker_bytes).map_err(|_| OperationError::Journal)?,
    )?;
    let view_bytes = source(root, &manifest, &config.graph.rendered)?;
    // Raw copied bytes were rehashed above. A Windows checkout may retain
    // CRLF for the canonical LF Git render already proven before Complete;
    // compare its rendering without rewriting any retained identity or bytes.
    let rendered = std::str::from_utf8(&view_bytes).map_err(|_| OperationError::Journal)?;
    if config.repository_id != fields.repository_id
        || workspace.workspace_id != fields.workspace_id
        || workspace.revision != manifest.graph_revision
        || workspace.stable_baseline_commit != baseline.as_str()
        || workspace.stable_branch != config.branches.stable
        || workspace.integration_branch != config.branches.integration
        || workspace.design_root != config.design.root
        || !workspace.ephemeral_workspace
        || !workspace.purge_before_stable_release
        || !config.plan.ephemeral
        || !config.plan.purge_before_stable_release
        || marker.repository_id != fields.repository_id
        || marker.schema_version != DesignMarker::SCHEMA_VERSION
        || marker.managed_by != DesignMarker::MANAGED_BY
        || rendered.replace("\r\n", "\n") != render_markdown(&workspace)?
    {
        return Err(OperationError::Refused(
            "retired terminal graph/configuration differs",
        ));
    }
    let mut plans = BTreeSet::new();
    for plan in &manifest.selected_plans {
        let p = plan.fields();
        let node = workspace.get(&p.plan_id).ok_or(OperationError::Journal)?;
        if !plans.insert(&p.plan_id)
            || p.repository_id != fields.repository_id
            || p.workspace_id != fields.workspace_id
            || p.plan_path.as_str() != node.path
            || p.branch != node.branch
            || p.lineage_id != node.lineage_id
            || node.status != crate::domain::status::PlanStatus::Accepted
            || p.body_sha256.as_ref()
                != Some(&digest(&source(root, &manifest, p.plan_path.as_str())?))
            || node
                .body_sha256
                .as_ref()
                .is_some_and(|body| Some(body) != p.body_sha256.as_ref())
        {
            return Err(OperationError::Refused("retired Plan attribution differs"));
        }
    }
    // Required retained evidence stays bound, but no present applicability or
    // candidate-checkout lookup is demanded of a historical completed release.
    let mut evidence = BTreeSet::new();
    for file in &fields.verification_evidence {
        if !evidence.insert(&file.path) {
            return Err(OperationError::Journal);
        }
        exact(root, file)?;
    }
    let evidence_root = regular_path(root, &bundle(&manifest.operation_id, "", "evidence")?)?;
    let prefix = format!(
        "{}/",
        bundle(&manifest.operation_id, "", "evidence")?.as_str()
    );
    for file in fields.verification_evidence.iter().skip(1) {
        let logical = file
            .path
            .as_str()
            .strip_prefix(&prefix)
            .ok_or(OperationError::Journal)?;
        if logical.ends_with(".json") {
            let stored: crate::domain::verification_receipt::StoredVerification =
                strict_json(&exact(root, file)?)?;
            stored.validate().map_err(|_| OperationError::Journal)?;
            if !manifest
                .selected_plans
                .contains(&stored.receipt.fields().plan)
            {
                return Err(OperationError::Journal);
            }
            let location = crate::infrastructure::verification_store::VerificationLocation::new(
                &evidence_root,
                &stored.receipt.fields().plan,
                &stored.receipt.fields().receipt_id,
            )?;
            if location.relative_path().as_str() != logical {
                return Err(OperationError::Journal);
            }
            for artifact in &stored.receipt.fields().artifacts {
                location.artifact(&artifact.path)?;
                let mut copied = artifact.clone();
                copied.path = bundle(&manifest.operation_id, "evidence/", artifact.path.as_str())?;
                exact(root, &copied)?;
            }
        } else if !logical.ends_with(".gate-claim") {
            return Err(OperationError::Journal);
        }
    }
    let current_bytes = read_bounded(&regular_path(root, &RepoPath::new(".mine/config.toml")?)?)?;
    let current = MineConfig::parse(
        Path::new(".mine/config.toml"),
        std::str::from_utf8(&current_bytes).map_err(|_| OperationError::Journal)?,
    )?;
    if current.repository_id != fields.repository_id {
        return Err(OperationError::Journal);
    }
    let mut git = checkpoint_git(root, None)?;
    let stable = git.text(&[
        "rev-parse",
        "--verify",
        &format!("refs/heads/{}^{{commit}}", current.branches.stable),
    ])?;
    git.validate_commits(&[integrated.as_str().into(), baseline.as_str().into()])?;
    if git.text(&["rev-parse", &format!("{}^{{tree}}", integrated.as_str())])?
        != manifest.curation.candidate_tree.as_str()
        || !git.is_ancestor(baseline.as_str(), integrated.as_str())?
        || !git.is_ancestor(integrated.as_str(), &stable)?
    {
        return Err(OperationError::Refused(
            "completed Release is not in actual stable history",
        ));
    }
    Ok(WorkspacePredecessor {
        workspace_id: fields.workspace_id.clone(),
        release_operation_id: fields.operation_id.clone(),
        terminal_stage_revision: fields.stage_revision,
        manifest: identity.clone(),
    })
}

/// `observed` must be the complete already-validated compact repository scan.
/// Only the explicitly active workspace may remain unsealed; all others must
/// form one succession chain and have retained, genuinely completed releases.
pub(crate) fn predecessors(
    journal: &RepositoryJournal<'_>,
    observed: &[OperationObservation],
    active: Option<&str>,
) -> OperationResult<Vec<WorkspacePredecessor>> {
    let mut groups = BTreeMap::<&str, Vec<&OperationObservation>>::new();
    for record in observed {
        groups
            .entry(&record.operation.fields().workspace_id)
            .or_default()
            .push(record);
    }
    let mut seals = BTreeMap::new();
    let mut initial = BTreeMap::new();
    for (workspace, records) in &groups {
        let roots = records
            .iter()
            .filter(|record| record.initial_predecessors.is_some())
            .collect::<Vec<_>>();
        if roots.len() > 1 {
            return Err(OperationError::Refused(
                "workspace UUID has multiple initial intents",
            ));
        }
        initial.insert(
            *workspace,
            roots.first().map(|record| {
                record
                    .initial_predecessors
                    .as_ref()
                    .expect("selected")
                    .clone()
            }),
        );
        if active == Some(*workspace) {
            continue;
        }
        if records.iter().any(|record| !terminal(&record.operation)) {
            return Err(OperationError::Refused(
                "unsealed historical workspace has pending operations",
            ));
        }
        let releases = records
            .iter()
            .filter(|record| {
                record.operation.fields().stage == OperationStage::Release(ReleaseStage::Complete)
                    && record.operation.fields().outcome == OperationOutcome::Committed
            })
            .collect::<Vec<_>>();
        if releases.len() != 1 {
            return Err(OperationError::Refused(
                "historical workspace needs exactly one completed Release",
            ));
        }
        let summary = releases[0];
        let record = journal.load_record(&summary.operation.fields().operation_id)?;
        if record.operation != summary.operation || record.closure != summary.closure {
            return Err(OperationError::Refused(
                "retired Release changed during observation",
            ));
        }
        seals.insert(*workspace, seal(journal.root(), &record)?);
    }
    validate_succession(&initial, &seals)?;
    Ok(seals.into_values().collect())
}

fn validate_succession(
    initial: &BTreeMap<&str, Option<Vec<WorkspacePredecessor>>>,
    seals: &BTreeMap<&str, WorkspacePredecessor>,
) -> OperationResult<()> {
    let mut ordered = initial.keys().copied().collect::<Vec<_>>();
    ordered.sort_by_key(|workspace| initial[workspace].as_ref().map_or(0, Vec::len));
    let mut consumed = BTreeMap::new();
    for workspace in ordered {
        let predecessors = initial[workspace].as_deref().unwrap_or(&[]);
        let actual = predecessors
            .iter()
            .map(|proof| (proof.workspace_id.as_str(), proof.clone()))
            .collect::<BTreeMap<_, _>>();
        if actual.len() != predecessors.len() || actual != consumed {
            return Err(OperationError::Refused(
                "workspace predecessor proof forks or omits repository history",
            ));
        }
        if let Some(proof) = seals.get(workspace) {
            consumed.insert(workspace, proof.clone());
        } else if initial.len() > 1 && initial[workspace].is_none() {
            return Err(OperationError::Refused(
                "active workspace has no initial predecessor binding",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof(workspace: &str, ordinal: u128) -> WorkspacePredecessor {
        let id = OperationId::parse(
            uuid::Uuid::from_u128(0x00000000_0000_4000_8000_000000000000 | ordinal).to_string(),
        )
        .unwrap();
        WorkspacePredecessor {
            workspace_id: workspace.into(),
            release_operation_id: id.clone(),
            terminal_stage_revision: 3,
            manifest: FileIdentity {
                path: bundle(&id, "", "manifest.json").unwrap(),
                bytes: 1,
                sha256: digest(b"fixture"),
            },
        }
    }

    #[test]
    fn succession_accepts_empty_legacy_root_and_two_frozen_successors() {
        validate_succession(&BTreeMap::new(), &BTreeMap::new()).unwrap();
        let a = proof("A", 1);
        let b = proof("B", 2);
        let seals = BTreeMap::from([("A", a.clone()), ("B", b.clone())]);
        validate_succession(
            &BTreeMap::from([
                ("A", None),
                ("B", Some(vec![a.clone()])),
                ("C", Some(vec![b, a])),
            ]),
            &seals,
        )
        .unwrap();
    }

    #[test]
    fn succession_refuses_fork_omission_and_rewritten_manifest_or_stage() {
        let a = proof("A", 1);
        let b = proof("B", 2);
        let seals = BTreeMap::from([("A", a.clone()), ("B", b)]);
        let fork = BTreeMap::from([
            ("A", None),
            ("B", Some(vec![a.clone()])),
            ("C", Some(vec![a.clone()])),
        ]);
        assert!(validate_succession(&fork, &seals).is_err());
        for mut predecessors in [vec![], vec![a.clone(), a.clone()], vec![a.clone()]] {
            if predecessors.len() == 1 {
                predecessors[0].terminal_stage_revision += 1;
            }
            let initial = BTreeMap::from([("A", None), ("B", Some(predecessors))]);
            assert!(validate_succession(&initial, &BTreeMap::from([("A", a.clone())])).is_err());
        }
        let mut changed = a.clone();
        changed.manifest.sha256 = digest(b"replacement");
        assert!(
            validate_succession(
                &BTreeMap::from([("A", None), ("B", Some(vec![changed]))]),
                &BTreeMap::from([("A", a)])
            )
            .is_err()
        );
    }
}
