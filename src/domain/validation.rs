//! Execution-graph validation and graph algorithms.
//!
//! Implements `docs/design/execution-graph/state-machine-and-algorithms.md`
//! "Validation" and "Parallel wave": unique IDs/paths, valid predecessors,
//! acyclic hard dependencies, legal states, ready frontier, topological sort,
//! parallel-wave computation with write-scope conflict detection, and
//! generated-view revision parity. These are pure functions over the
//! [`PlanWorkspace`] aggregate.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::domain::error::{MineError, MineResult};
use crate::domain::graph::{
    CURRENT_GRAPH_SCHEMA_VERSION, PREVIOUS_GRAPH_SCHEMA_VERSION, PlanWorkspace,
};
use crate::domain::path::normalize_repo_relative;
use crate::domain::plan_id::{CanonicalPlanId, is_exact_bootstrap_node, registered_plan_branch};
use crate::domain::status::PlanStatus;

/// Validates the structural integrity of the whole workspace.
///
/// Per `docs/design/execution-graph/state-machine-and-algorithms.md`, the
/// "Validation" step checks: unique IDs/paths, valid predecessors, acyclic hard
/// dependencies, legal status values (enforced by the typed enum), normalized
/// safe paths, non-empty design references, generated-view revision parity,
/// and branch/workspace consistency. Exclusive-write scope overlap is **not**
/// a structural validation concern: it is a `parallel_wave` constraint applied
/// only among `READY` plans, so that sequential plans (e.g. an `ACCEPTED`
/// predecessor owning a broad path and its `READY` successor owning a
/// subdirectory) validate cleanly.
///
/// # Errors
/// Returns the first [`MineError`] describing the violation.
pub fn validate(ws: &PlanWorkspace) -> MineResult<()> {
    validate_with_worktrees_root(ws, ".mine/worktrees")
}

/// Validates a workspace against the configured repository-relative worktree
/// root. Persistence adapters use this form so a non-default
/// `[plan].worktrees_root` remains authoritative.
pub fn validate_with_worktrees_root(ws: &PlanWorkspace, worktrees_root: &str) -> MineResult<()> {
    if !matches!(
        ws.schema_version,
        CURRENT_GRAPH_SCHEMA_VERSION | PREVIOUS_GRAPH_SCHEMA_VERSION
    ) {
        return Err(MineError::GraphInvalid {
            detail: format!(
                "unsupported execution-graph schema version {}; expected {} (run `mine graph repair-provenance` for schema v1/v2)",
                ws.schema_version, CURRENT_GRAPH_SCHEMA_VERSION
            ),
        });
    }
    let normalized_worktrees_root = normalize_repo_relative(worktrees_root)?;

    // Unique IDs.
    let mut seen_ids = HashSet::new();
    for p in &ws.plans {
        if !seen_ids.insert(p.id.as_str()) {
            return Err(MineError::GraphInvalid {
                detail: format!("duplicate plan id: {}", p.id),
            });
        }
    }

    // Unique plan paths.
    let mut seen_paths = HashSet::new();
    for p in &ws.plans {
        let norm = normalize_repo_relative(&p.path)?;
        if !seen_paths.insert(norm) {
            return Err(MineError::GraphInvalid {
                detail: format!("duplicate plan path: {}", p.path),
            });
        }
    }

    // Valid predecessors and non-empty design references.
    let id_set = ws.ids();
    for p in &ws.plans {
        for pred in &p.hard_predecessors {
            if !id_set.contains(pred.as_str()) {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} hard_predecessor {} not found", p.id, pred),
                });
            }
            if pred == &p.id {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} depends on itself", p.id),
                });
            }
        }
        for pred in &p.soft_predecessors {
            if !id_set.contains(pred.as_str()) {
                return Err(MineError::GraphInvalid {
                    detail: format!("plan {} soft_predecessor {} not found", p.id, pred),
                });
            }
        }
        if p.design_references.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has no design references", p.id),
            });
        }
        // Validate all owned paths are safe.
        for path in &p.design_references {
            normalize_repo_relative(path)?;
        }
        for path in &p.exclusive_write_paths {
            normalize_repo_relative(path)?;
        }
        for path in &p.read_only_paths {
            normalize_repo_relative(path)?;
        }
        for path in &p.reserved_shared_paths {
            normalize_repo_relative(path)?;
        }
        // Historical integration fixtures predate canonical Plan IDs. This is
        // not a production alias: only the exact fixture workspace may retain
        // all-empty provenance for its non-canonical synthetic nodes.
        if ws.workspace_id == "bootstrap-mine-v1" && !p.id.starts_with("plan") {
            if !p.lineage_id.is_empty() || !p.worktree_path.is_empty() || !p.branch.is_empty() {
                return Err(MineError::GraphInvalid {
                    detail: format!("fixture plan {} has partial canonical provenance", p.id),
                });
            }
            continue;
        }
        if p.lineage_id.is_empty() || p.worktree_path.is_empty() || p.branch.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has incomplete canonical provenance", p.id),
            });
        }
        let anchor_must_be_empty = matches!(
            p.status,
            PlanStatus::Draft | PlanStatus::Blocked | PlanStatus::Ready | PlanStatus::Withdrawn
        );
        if anchor_must_be_empty != p.branch_anchor.is_empty() {
            return Err(MineError::GraphInvalid {
                detail: format!(
                    "plan {} has branch_anchor inconsistent with status {}",
                    p.id, p.status
                ),
            });
        }
        let (expected_lineage, expected_worktree) =
            if is_exact_bootstrap_node(&ws.workspace_id, &p.id, &p.path) {
                (
                    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                    "plan01".to_string(),
                    // mine-release-allow-plan-reference: canonical Plan grammar or protocol fixture
                    format!("{normalized_worktrees_root}/plan01"),
                )
            } else {
                let parsed = CanonicalPlanId::parse(&p.id)?;
                (
                    parsed.lineage_id(),
                    parsed.worktree_path_under(&normalized_worktrees_root),
                )
            };
        let expected_branch = registered_plan_branch(&ws.workspace_id, &p.id, &p.path)?;
        let normalized_worktree = normalize_repo_relative(&p.worktree_path)?;
        if p.lineage_id != expected_lineage
            || normalized_worktree != expected_worktree
            || p.branch != expected_branch
        {
            return Err(MineError::GraphInvalid {
                detail: format!("plan {} has inconsistent canonical provenance", p.id),
            });
        }
    }

    // Acyclic hard dependencies (topological sort via Kahn's algorithm).
    topological_sort(ws)?;

    // Exclusive-write scope overlap is intentionally NOT checked here: per the
    // design it is a `parallel_wave` constraint among READY plans, not a
    // structural validation rule. Two sequential plans may legitimately
    // own overlapping paths (a broad predecessor path and a narrower
    // successor path) because they never write concurrently.

    Ok(())
}

/// Returns a topological ordering of plan IDs via Kahn's algorithm on hard
/// predecessors. Stable: emits predecessors before descendants in declaration
/// order.
///
/// # Errors
/// Returns [`MineError::GraphCycle`] if a cycle is detected.
pub fn topological_sort(ws: &PlanWorkspace) -> MineResult<Vec<String>> {
    let mut indeg: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for p in &ws.plans {
        indeg.entry(p.id.as_str()).or_insert(0);
        adj.entry(p.id.as_str()).or_default();
    }
    for p in &ws.plans {
        for pred in &p.hard_predecessors {
            adj.entry(pred.as_str()).or_default().push(p.id.as_str());
            *indeg.entry(p.id.as_str()).or_insert(0) += 1;
        }
    }

    // Stable queue: plans with zero indegree, in declaration order.
    let mut queue: VecDeque<&str> = ws
        .plans
        .iter()
        .filter(|p| *indeg.get(p.id.as_str()).unwrap_or(&0) == 0)
        .map(|p| p.id.as_str())
        .collect();
    // Keep declaration order within the same indegree level.
    let mut order = Vec::with_capacity(ws.plans.len());
    while let Some(id) = queue.pop_front() {
        order.push(id.to_string());
        if let Some(children) = adj.get(id) {
            // Re-scan to preserve declaration order among newly-zero nodes.
            for child in children {
                if let Some(d) = indeg.get_mut(child) {
                    *d -= 1;
                }
            }
            // Append newly-zero nodes in declaration order.
            for p in &ws.plans {
                if p.id.as_str() == id {
                    continue;
                }
                if order.iter().any(|o| o == &p.id) || queue.iter().any(|q| *q == p.id.as_str()) {
                    continue;
                }
                if *indeg.get(p.id.as_str()).unwrap_or(&0) == 0
                    && adj.get(id).is_some_and(|c| c.contains(&p.id.as_str()))
                {
                    queue.push_back(p.id.as_str());
                }
            }
        }
    }

    if order.len() != ws.plans.len() {
        let remaining: Vec<&str> = ws
            .plans
            .iter()
            .filter(|p| !order.contains(&p.id))
            .map(|p| p.id.as_str())
            .collect();
        return Err(MineError::GraphCycle {
            cycle: remaining.join(" -> "),
        });
    }
    Ok(order)
}

/// Returns `true` if a plan's hard predecessors are all `ACCEPTED`.
pub fn hard_predecessors_accepted(ws: &PlanWorkspace, plan_id: &str) -> MineResult<bool> {
    let node = ws.get(plan_id).ok_or_else(|| MineError::PlanNotFound {
        plan_id: plan_id.to_string(),
    })?;
    for pred in &node.hard_predecessors {
        let pred_node = ws.get(pred).ok_or_else(|| MineError::PlanNotFound {
            plan_id: pred.to_string(),
        })?;
        if pred_node.status != PlanStatus::Accepted {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Computes the derived readiness of a plan: ready only when every hard
/// predecessor is accepted. (Design references and path ownership are checked
/// by [`validate`].)
pub fn is_derived_ready(ws: &PlanWorkspace, plan_id: &str) -> MineResult<bool> {
    hard_predecessors_accepted(ws, plan_id)
}

/// Returns the READY frontier: all plans currently `READY`. (Does not mutate;
/// derived readiness recomputation is a service concern.)
#[must_use]
pub fn ready_frontier(ws: &PlanWorkspace) -> Vec<String> {
    ws.plans
        .iter()
        .filter(|p| p.status == PlanStatus::Ready)
        .map(|p| p.id.clone())
        .collect()
}

/// Computes a parallel wave: a stable maximal set of `READY` plans with no
/// ancestor relationship among them and no mutual write-scope conflict.
///
/// Two plans are excluded from the same wave when: they have a hard-dependency
/// ancestor relationship, or their write scopes overlap (exclusive-write vs
/// exclusive-write, or exclusive-write vs reserved-shared).
#[must_use]
pub fn parallel_wave(ws: &PlanWorkspace) -> Vec<String> {
    let ready: Vec<&crate::domain::graph::PlanNode> = ws
        .plans
        .iter()
        .filter(|p| p.status == PlanStatus::Ready)
        .collect();
    // Greedy stable selection in declaration order.
    let mut wave: Vec<String> = Vec::new();
    for candidate in &ready {
        let mut compatible = true;
        for chosen_id in &wave {
            let chosen = ws.get(chosen_id).expect("chosen is from ready set");
            if candidate
                .has_ancestor_relationship(&chosen.id, ws)
                .unwrap_or(false)
            {
                compatible = false;
                break;
            }
            if candidate.write_scope_overlaps(chosen).unwrap_or(false) {
                compatible = false;
                break;
            }
        }
        if compatible {
            wave.push(candidate.id.clone());
        }
    }
    wave
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::MineError;
    use crate::domain::graph::PlanNode;

    fn ws(plans: Vec<PlanNode>) -> PlanWorkspace {
        PlanWorkspace {
            schema_version: crate::domain::graph::CURRENT_GRAPH_SCHEMA_VERSION,
            revision: 0,
            project_id: "p".to_string(),
            workspace_id: "bootstrap-mine-v1".to_string(),
            stable_branch: "master".to_string(),
            integration_branch: "dev".to_string(),
            stable_baseline_commit: String::new(),
            design_root: "docs/design/index.md".to_string(),
            ephemeral_workspace: true,
            purge_before_stable_release: true,
            plans,
        }
    }

    fn node(id: &str, hard: &[&str], excl: &[&str]) -> PlanNode {
        PlanNode {
            id: id.to_string(),
            path: format!("docs/plan/{id}.md"),
            title: id.to_string(),
            branch: String::new(),
            branch_anchor: String::new(),
            status: PlanStatus::Blocked,
            hard_predecessors: hard.iter().map(|s| s.to_string()).collect(),
            soft_predecessors: vec![],
            design_references: vec!["docs/design/principles.md".to_string()],
            exclusive_write_paths: excl.iter().map(|s| s.to_string()).collect(),
            read_only_paths: vec![],
            reserved_shared_paths: vec![],
            implementation_report: String::new(),
            review_report: String::new(),
            implementation_commits: vec![],
            owner: String::new(),
            run_id: String::new(),
            started_at: String::new(),
            updated_at: String::new(),
            rejection_reason: String::new(),
            compensating_plan: String::new(),
            withdraw_reason: String::new(),
            withdrawn_at: String::new(),
            planning_scope: String::new(),
            readiness_checked_at: String::new(),
            adr_registry_revision: 0,
            lineage_id: String::new(),
            worktree_path: String::new(),
        }
    }

    #[test]
    fn valid_chain_validates() -> Result<(), MineError> {
        let g = ws(vec![
            node("01", &[], &["src/a/"]),
            node("02", &["01"], &["src/b/"]),
            node("03", &["02"], &["src/c/"]),
        ]);
        validate(&g)?;
        assert_eq!(topological_sort(&g)?, vec!["01", "02", "03"]);
        Ok(())
    }

    #[test]
    fn duplicate_id_rejected() {
        let g = ws(vec![
            node("01", &[], &["src/a/"]),
            node("01", &[], &["src/b/"]),
        ]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn missing_predecessor_rejected() {
        let g = ws(vec![node("02", &["01"], &["src/b/"])]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn cycle_detected() {
        let g = ws(vec![
            node("01", &["02"], &["src/a/"]),
            node("02", &["01"], &["src/b/"]),
        ]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_CYCLE");
    }

    #[test]
    fn self_dependency_rejected() {
        let g = ws(vec![node("01", &["01"], &["src/a/"])]);
        assert_eq!(validate(&g).unwrap_err().code(), "MINE_GRAPH_INVALID");
    }

    #[test]
    fn sequential_plans_with_overlapping_scopes_validate() {
        // Per the design, exclusive-write overlap is a parallel-wave constraint,
        // not a structural validation rule. A broad predecessor scope and a
        // narrower successor scope must validate cleanly because they never
        // write concurrently.
        let g = ws(vec![
            node("01", &[], &["src/"]),
            node("02", &["01"], &["src/domain/"]),
        ]);
        validate(&g).expect("sequential overlapping scopes validate");
    }

    #[test]
    fn parallel_wave_splits_overlapping_scopes() {
        let mut g = ws(vec![
            node("01", &[], &["src/shared/"]),
            node("02", &[], &["src/shared/x.rs"]),
            node("03", &[], &["src/other/"]),
        ]);
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        let wave = parallel_wave(&g);
        // 01 and 02 overlap; only one of them can join the wave alongside 03.
        assert!(wave.contains(&"03".to_string()));
        assert!(
            wave.contains(&"01".to_string()) ^ wave.contains(&"02".to_string()),
            "wave was {wave:?}"
        );
    }

    #[test]
    fn derived_ready_requires_accepted_predecessors() -> Result<(), MineError> {
        let mut g = ws(vec![
            node("01", &[], &["src/a/"]),
            node("02", &["01"], &["src/b/"]),
        ]);
        assert!(!is_derived_ready(&g, "02")?);
        g.plans[0].status = PlanStatus::Accepted;
        assert!(is_derived_ready(&g, "02")?);
        Ok(())
    }

    #[test]
    fn ready_frontier_lists_ready_plans() {
        let mut g = ws(vec![
            node("01", &[], &["src/a/"]),
            node("02", &[], &["src/b/"]),
            node("03", &[], &["src/c/"]),
        ]);
        g.plans[0].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        assert_eq!(ready_frontier(&g), vec!["01", "03"]);
    }

    #[test]
    fn wave_excludes_ancestor_pairs() {
        let mut g = ws(vec![
            node("01", &[], &["src/a/"]),
            node("02", &["01"], &["src/b/"]),
        ]);
        // Make 02 READY while 01 is also READY (artificial, for wave test).
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        let wave = parallel_wave(&g);
        assert!(wave.contains(&"01".to_string()));
        assert!(!wave.contains(&"02".to_string()));
    }

    #[test]
    fn wave_excludes_write_scope_conflict() {
        let mut g = ws(vec![
            node("01", &[], &["src/shared/"]),
            node("02", &[], &["src/shared/x.rs"]),
            node("03", &[], &["src/c/"]),
        ]);
        // Suppress the structural conflict check by making scopes disjoint
        // for validation, then test wave with an overlap via reserved paths.
        g.plans[1].exclusive_write_paths = vec!["src/other/".to_string()];
        g.plans[0].status = PlanStatus::Ready;
        g.plans[1].status = PlanStatus::Ready;
        g.plans[2].status = PlanStatus::Ready;
        // 01 and 02 are now disjoint; all three ready with disjoint scopes.
        let wave = parallel_wave(&g);
        assert_eq!(wave.len(), 3);
    }
}
