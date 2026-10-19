//! Explicit isolated configuration, independent of the source checkout's layout.
use mine::domain::config::{
    AdrConfig, BranchesConfig, DesignConfig, GovernanceLayout, GraphConfig, MineConfig, PlanConfig,
};

pub fn canonical() -> MineConfig {
    let layout = GovernanceLayout::canonical();
    MineConfig {
        schema_version: 1,
        repository_id: uuid::Uuid::new_v4().to_string(),
        mine_code_version: env!("CARGO_PKG_VERSION").into(),
        branches: BranchesConfig {
            stable: "master".into(),
            integration: "dev".into(),
        },
        design: DesignConfig {
            root: layout.design_index().as_str().into(),
            marker: layout.design_marker().as_str().into(),
            language: "en".into(),
            index_soft_limit_lines: 250,
            leaf_soft_limit_lines: 400,
        },
        plan: PlanConfig {
            root: layout.plan_dir().as_str().into(),
            ephemeral: true,
            purge_before_stable_release: true,
            worktrees_root: ".mine/worktrees".into(),
            shared_environment_root: ".".into(),
        },
        graph: GraphConfig {
            source: layout.graph_source().as_str().into(),
            rendered: layout.graph_view().as_str().into(),
            lock_timeout_ms: 5000,
        },
        adr: AdrConfig::default(),
    }
}
