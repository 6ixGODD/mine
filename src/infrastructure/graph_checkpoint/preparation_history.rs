//! Preparation policy and complete graph-only history, acquired in batches.
//! This is a request-local proof, never a persistent Git validation cache.
use super::digest;
use crate::domain::{config::MineConfig, graph::*, lifecycle_operation::*, validation};
use crate::infrastructure::{git_evidence::GitRequest, toml_store::render_markdown};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Closed public configuration facet needed for a pure policy witness. The
/// outer runtime binds the actual config hash; live effects rederive this facet
/// from that exact configuration. Unknown or secret config bytes are not copied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationAuthority {
    pub repository: String,
    pub stable: String,
    pub integration: String,
    pub source: RepoPath,
    pub view: RepoPath,
    pub plan_root: RepoPath,
    pub worktrees_root: RepoPath,
    pub shared_environment_root: String,
}
impl PreparationAuthority {
    pub fn observe(config: &MineConfig) -> OperationResult<Self> {
        Ok(Self {
            repository: config.repository_id.clone(),
            stable: config.branches.stable.clone(),
            integration: config.branches.integration.clone(),
            source: RepoPath::new(config.graph.source.clone())?,
            view: RepoPath::new(config.graph.rendered.clone())?,
            plan_root: RepoPath::new(config.plan.root.clone())?,
            worktrees_root: RepoPath::new(config.plan.worktrees_root.clone())?,
            shared_environment_root: config.plan.shared_environment_root.clone(),
        })
    }
}

/// Only preparation-relevant authority is hashed. Status, owner/run, reports,
/// readiness timestamps, graph revision and other nodes' occupancy are absent.
pub(crate) fn policy(
    config: &MineConfig,
    workspace: &PlanWorkspace,
    node: &PlanNode,
    body: &Sha256,
    decisions: &[FileIdentity],
) -> OperationResult<Sha256> {
    policy_for(
        &PreparationAuthority::observe(config)?,
        workspace,
        node,
        body,
        decisions,
    )
}

pub(crate) fn policy_for(
    authority: &PreparationAuthority,
    workspace: &PlanWorkspace,
    node: &PlanNode,
    body: &Sha256,
    decisions: &[FileIdentity],
) -> OperationResult<Sha256> {
    #[derive(Serialize)]
    struct Policy<'a> {
        contract: &'static str,
        workspace: &'a str,
        authority: &'a PreparationAuthority,
        identity: [&'a str; 6],
        original_body: &'a Option<Sha256>,
        effective_body: &'a Sha256,
        dependencies: [&'a [String]; 2],
        scopes: [&'a [String]; 4],
        ownership: &'a Option<crate::domain::ownership::OwnershipContract>,
        approval: &'a Option<OwnershipPolicyProvenance>,
        clarifications: &'a [crate::domain::ownership::ClarificationRecord],
        decisions: &'a [FileIdentity],
    }
    if decisions.len() != node.design_references.len()
        || decisions
            .iter()
            .zip(&node.design_references)
            .any(|(a, b)| a.path.as_str() != b)
    {
        return Err(OperationError::Refused(
            "preparation decision identities differ",
        ));
    }
    let binding = Policy {
        contract: "mine-ready-preparation-v1",
        workspace: &workspace.workspace_id,
        authority,
        identity: [
            &node.id,
            &node.path,
            &node.branch,
            &node.lineage_id,
            &node.worktree_path,
            &node.planning_scope,
        ],
        original_body: &node.body_sha256,
        effective_body: body,
        dependencies: [&node.hard_predecessors, &node.soft_predecessors],
        scopes: [
            &node.design_references,
            &node.exclusive_write_paths,
            &node.read_only_paths,
            &node.reserved_shared_paths,
        ],
        ownership: &node.ownership,
        approval: &node.ownership_policy,
        clarifications: &node.clarifications,
        decisions,
    };
    Ok(digest(
        &serde_json::to_vec(&binding).map_err(|_| OperationError::Journal)?,
    ))
}

/// A complete single-parent chain, not endpoint equality or a path-filtered
/// ancestry query. All children are bounded by the caller's existing request.
pub(crate) fn prove(
    git: &mut GitRequest,
    config: &MineConfig,
    current: &PlanWorkspace,
    node: &PlanNode,
    root_head: &CommitId,
    decisions: &[FileIdentity],
) -> OperationResult<()> {
    let generation = node
        .preparations
        .last()
        .ok_or(OperationError::Refused("preparation absent"))?;
    let body = generation
        .plan
        .fields()
        .body_sha256
        .as_ref()
        .ok_or(OperationError::Journal)?;
    if policy(config, current, node, body, decisions)? != generation.policy_sha256 {
        return Err(OperationError::Refused(
            "preparation policy changed; explicit reprepare required",
        ));
    }
    if git.text(&["rev-parse", "--is-shallow-repository"])? != "false"
        || !git
            .run(
                &["for-each-ref", "--format=%(refname)", "refs/replace/"],
                b"",
            )?
            .is_empty()
    {
        return Err(OperationError::Refused(
            "shallow/replaced preparation history",
        ));
    }
    let graft = git.text(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-path",
        "info/grafts",
    ])?;
    if !std::path::Path::new(&graft).is_absolute() || std::path::Path::new(&graft).try_exists()? {
        return Err(OperationError::Refused(
            "unsupported preparation graft history",
        ));
    }
    let range = format!(
        "{}..{}",
        generation.observed_root_commit.as_str(),
        root_head.as_str()
    );
    let parents = git.run(&["rev-list", "--parents", "--reverse", &range, "--"], b"")?;
    let chain = linear_chain(&parents, &generation.observed_root_commit, root_head)?;
    let mut commits = vec![generation.observed_root_commit.as_str().to_owned()];
    commits.extend(chain.iter().map(|id| id.as_str().to_owned()));
    git.validate_commits(&commits)?;
    let mut input = Vec::new();
    for id in &chain {
        input.extend_from_slice(id.as_str().as_bytes());
        input.push(b'\n');
    }
    // No pathspec: a product change followed by revert is rejected at its
    // intermediate full-tree delta, even when the endpoints match.
    let raw = git.run(
        &[
            "diff-tree",
            "--stdin",
            "--raw",
            "-r",
            "-z",
            "--no-renames",
            "--no-abbrev",
            "--no-ext-diff",
            "--no-color",
            "--no-abbrev-commit",
        ],
        &input,
    )?;
    let source = RepoPath::new(config.graph.source.clone())?;
    let view = RepoPath::new(config.graph.rendered.clone())?;
    let pairs = graph_pairs(&raw, &chain, [&source, &view])?;
    let mut unique = BTreeSet::new();
    for pair in &pairs {
        unique.extend(pair.iter().map(|id| id.as_str().to_owned()));
    }
    let ids = unique
        .into_iter()
        .map(CommitId::new)
        .collect::<Result<Vec<_>, _>>()?;
    let mut input = Vec::new();
    for id in &ids {
        input.extend_from_slice(id.as_str().as_bytes());
        input.push(b'\n');
    }
    let objects = git.run(&["cat-file", "--batch"], &input)?;
    let blobs = blobs(&objects, &ids)?;
    let mut previous = None;
    let mut anchors = Vec::new();
    for (index, pair) in pairs.iter().enumerate() {
        let source = blobs.get(&pair[0]).ok_or(OperationError::Journal)?;
        let view = blobs.get(&pair[1]).ok_or(OperationError::Journal)?;
        let graph: PlanWorkspace =
            toml::from_str(std::str::from_utf8(source).map_err(|_| OperationError::Journal)?)
                .map_err(|_| OperationError::Journal)?;
        validation::validate_with_worktrees_root(&graph, &config.plan.worktrees_root)?;
        if graph.workspace_id != current.workspace_id
            || graph.project_id != current.project_id
            || graph.stable_baseline_commit != current.stable_baseline_commit
            || graph.stable_branch != current.stable_branch
            || graph.integration_branch != current.integration_branch
            || graph.design_root != current.design_root
            || graph.ephemeral_workspace != current.ephemeral_workspace
            || graph.purge_before_stable_release != current.purge_before_stable_release
            || render_markdown(&graph)?.as_bytes() != *view
            || (index == 0 && graph.revision != generation.observed_graph_revision)
        {
            return Err(OperationError::Refused(
                "preparation history graph/render identity differs",
            ));
        }
        let subject = graph.get(&node.id).ok_or(OperationError::Journal)?;
        if policy(config, &graph, subject, body, decisions)? != generation.policy_sha256 {
            return Err(OperationError::Refused(
                "intermediate preparation contract changed",
            ));
        }
        if let Some(before) = &previous {
            let before: &PlanWorkspace = before;
            if graph.revision <= before.revision {
                return Err(OperationError::Refused("history revision did not increase"));
            }
            validation::validate_native_successor(before, &graph)?;
        }
        anchors.extend(
            graph
                .plans
                .iter()
                .filter(|p| !p.branch_anchor.is_empty())
                .map(|p| p.branch_anchor.clone()),
        );
        previous = Some(graph);
    }
    if previous.as_ref() != Some(current) {
        return Err(OperationError::Refused(
            "prepared Root history does not reach current graph",
        ));
    }
    git.validate_commits(&anchors)?;
    Ok(())
}

fn linear_chain(bytes: &[u8], base: &CommitId, head: &CommitId) -> OperationResult<Vec<CommitId>> {
    let text = std::str::from_utf8(bytes).map_err(|_| OperationError::Journal)?;
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(OperationError::Journal);
    }
    let mut previous = base.clone();
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    for line in text.lines() {
        let fields = line.split(' ').collect::<Vec<_>>();
        if fields.len() != 2 || fields[1] != previous.as_str() {
            return Err(OperationError::Refused(
                "preparation history is not complete single-parent ancestry",
            ));
        }
        let id = CommitId::new(fields[0])?;
        if id == previous || !seen.insert(id.clone()) {
            return Err(OperationError::Journal);
        }
        previous = id.clone();
        chain.push(id);
    }
    if &previous != head {
        return Err(OperationError::Refused("preparation Root history diverged"));
    }
    Ok(chain)
}

/// Each batch row binds the actual old/new blob IDs of BOTH graph files.
/// Modes, renames, extra paths, omitted or duplicate commits all fail closed.
fn graph_pairs(
    raw: &[u8],
    chain: &[CommitId],
    paths: [&RepoPath; 2],
) -> OperationResult<Vec<[CommitId; 2]>> {
    if chain.is_empty() {
        return Err(OperationError::Refused(
            "completed preparation checkpoint not in Root history",
        ));
    }
    if raw.last() != Some(&0) {
        return Err(OperationError::Journal);
    }
    let mut parts = raw.split(|b| *b == 0);
    let mut pairs = Vec::new();
    for commit in chain {
        if parts.next() != Some(commit.as_str().as_bytes()) {
            return Err(OperationError::Journal);
        }
        let mut before: [Option<CommitId>; 2] = [None, None];
        let mut after: [Option<CommitId>; 2] = [None, None];
        for _ in 0..2 {
            let metadata = std::str::from_utf8(parts.next().ok_or(OperationError::Journal)?)
                .map_err(|_| OperationError::Journal)?;
            let fields = metadata.split(' ').collect::<Vec<_>>();
            let path = parts.next().ok_or(OperationError::Journal)?;
            let position = paths
                .iter()
                .position(|p| p.as_str().as_bytes() == path)
                .ok_or(OperationError::Refused(
                    "non-graph Root commit requires explicit reprepare",
                ))?;
            if fields.len() != 5
                || fields[0] != ":100644"
                || fields[1] != "100644"
                || fields[4] != "M"
                || before[position].is_some()
            {
                return Err(OperationError::Refused(
                    "history is not an exact regular graph pair delta",
                ));
            }
            before[position] = Some(CommitId::new(fields[2])?);
            after[position] = Some(CommitId::new(fields[3])?);
        }
        let before = [
            before[0].take().ok_or(OperationError::Journal)?,
            before[1].take().ok_or(OperationError::Journal)?,
        ];
        let after = [
            after[0].take().ok_or(OperationError::Journal)?,
            after[1].take().ok_or(OperationError::Journal)?,
        ];
        if let Some(last) = pairs.last() {
            if last != &before {
                return Err(OperationError::Journal);
            }
        } else {
            pairs.push(before);
        }
        pairs.push(after);
    }
    if parts.next() != Some(b"".as_slice()) || parts.next().is_some() {
        return Err(OperationError::Journal);
    }
    Ok(pairs)
}

fn blobs<'a>(
    mut bytes: &'a [u8],
    ids: &[CommitId],
) -> OperationResult<HashMap<CommitId, &'a [u8]>> {
    let mut result = HashMap::new();
    for id in ids {
        let newline = bytes
            .iter()
            .position(|b| *b == b'\n')
            .ok_or(OperationError::Journal)?;
        let header = std::str::from_utf8(&bytes[..newline]).map_err(|_| OperationError::Journal)?;
        let fields = header.split(' ').collect::<Vec<_>>();
        if fields.len() != 3 || fields[0] != id.as_str() || fields[1] != "blob" {
            return Err(OperationError::Journal);
        }
        let size: usize = fields[2].parse().map_err(|_| OperationError::Journal)?;
        bytes = &bytes[newline + 1..];
        if bytes.get(size) != Some(&b'\n') {
            return Err(OperationError::Journal);
        }
        result.insert(id.clone(), &bytes[..size]);
        bytes = &bytes[size + 1..];
    }
    if !bytes.is_empty() {
        return Err(OperationError::Journal);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: char) -> CommitId {
        CommitId::new(byte.to_string().repeat(40)).unwrap()
    }

    #[test]
    fn chain_requires_every_actual_single_parent_and_exact_endpoints() {
        let (a, b, c) = (id('a'), id('b'), id('c'));
        let chain = format!(
            "{} {}\n{} {}\n",
            b.as_str(),
            a.as_str(),
            c.as_str(),
            b.as_str()
        );
        assert_eq!(
            linear_chain(chain.as_bytes(), &a, &c).unwrap(),
            vec![b.clone(), c.clone()]
        );
        for malformed in [
            format!("{} {} {}\n", c.as_str(), a.as_str(), b.as_str()),
            format!("{} {}\n", c.as_str(), b.as_str()),
            format!("{} {}", c.as_str(), a.as_str()),
            format!(
                "{} {}\n{} {}\n",
                b.as_str(),
                a.as_str(),
                b.as_str(),
                b.as_str()
            ),
        ] {
            assert!(linear_chain(malformed.as_bytes(), &a, &c).is_err());
        }
        assert!(linear_chain(b"", &a, &c).is_err());
    }

    #[test]
    fn full_tree_delta_has_exact_pair_modes_paths_and_batch_framing() {
        let paths = [
            RepoPath::new("docs/plan/graph.toml").unwrap(),
            RepoPath::new("docs/plan/graph.md").unwrap(),
        ];
        let (commit, a, b, c, d) = (id('e'), id('a'), id('b'), id('c'), id('d'));
        let raw = format!(
            "{}\0:100644 100644 {} {} M\0{}\0:100644 100644 {} {} M\0{}\0",
            commit.as_str(),
            a.as_str(),
            c.as_str(),
            paths[0].as_str(),
            b.as_str(),
            d.as_str(),
            paths[1].as_str()
        );
        assert_eq!(
            graph_pairs(
                raw.as_bytes(),
                std::slice::from_ref(&commit),
                [&paths[0], &paths[1]]
            )
            .unwrap(),
            vec![[a, b], [c, d]]
        );
        for changed in [
            raw.replace("100644 100644", "100644 100755"),
            raw.replace(paths[1].as_str(), "src/product.rs"),
            raw.trim_end_matches('\0').into(),
            raw.clone() + "extra\0",
        ] {
            assert!(
                graph_pairs(
                    changed.as_bytes(),
                    std::slice::from_ref(&commit),
                    [&paths[0], &paths[1]]
                )
                .is_err()
            );
        }
    }

    #[test]
    fn blob_batches_are_exact_id_type_size_and_byte_frames_not_line_parsing() {
        let oid = id('a');
        let valid = format!("{} blob 3\nx\ny\n", oid.as_str());
        let objects = blobs(valid.as_bytes(), std::slice::from_ref(&oid)).unwrap();
        assert_eq!(objects.get(&oid).copied(), Some(b"x\ny".as_slice()));
        for invalid in [
            valid.replace("blob", "commit"),
            valid.replace("blob 3", "blob 4"),
            valid.clone() + "extra",
            format!("{} missing\n", oid.as_str()),
            format!("{} blob 0\n\n", id('b').as_str()),
        ] {
            assert!(blobs(invalid.as_bytes(), std::slice::from_ref(&oid)).is_err());
        }
    }

    #[test]
    fn policy_excludes_occupancy_and_revision_but_binds_contract_and_raw_body() {
        let mut workspace: PlanWorkspace = toml::from_str(include_str!(
            "../../../tests/fixtures/development-execution-graph.toml"
        ))
        .unwrap();
        let mut node = workspace.plans[0].clone();
        let authority = PreparationAuthority {
            repository: "fixture".into(),
            stable: "master".into(),
            integration: "dev".into(),
            source: RepoPath::new("docs/plan/execution-graph.toml").unwrap(),
            view: RepoPath::new("docs/plan/execution-graph.md").unwrap(),
            plan_root: RepoPath::new("docs/plan").unwrap(),
            worktrees_root: RepoPath::new(".mine/worktrees").unwrap(),
            shared_environment_root: ".".into(),
        };
        let decisions = node
            .design_references
            .iter()
            .map(|path| FileIdentity {
                path: RepoPath::new(path.clone()).unwrap(),
                bytes: 5,
                sha256: digest(b"fixed"),
            })
            .collect::<Vec<_>>();
        let body = digest(b"raw immutable body\r\n");
        let original = policy_for(&authority, &workspace, &node, &body, &decisions).unwrap();
        workspace.revision += 1;
        workspace.plans[1].owner = "other owner".into();
        node.status = crate::domain::status::PlanStatus::Ready;
        node.owner = "not an actor source".into();
        node.updated_at = "ordinary graph churn".into();
        node.readiness_checked_at = "new readiness observation".into();
        node.adr_registry_revision += 1;
        assert_eq!(
            policy_for(&authority, &workspace, &node, &body, &decisions).unwrap(),
            original
        );
        // mine-release-allow-plan-reference: isolated graph/protocol fixture, not repository Plan provenance
        node.hard_predecessors.push("plan99".into());
        assert_ne!(
            policy_for(&authority, &workspace, &node, &body, &decisions).unwrap(),
            original
        );
        node.hard_predecessors.pop();
        assert_ne!(
            policy_for(
                &authority,
                &workspace,
                &node,
                &digest(b"raw immutable body\n"),
                &decisions
            )
            .unwrap(),
            original
        );
    }
}
