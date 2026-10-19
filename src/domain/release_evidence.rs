//! Frozen data shared by active Release and completed-workspace history.
//! Decoding these observations never constructs checked curation authority.
use super::{graph::*, lifecycle_operation::OperationId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Blob,
    Tree,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeEntry {
    pub kind: EntryKind,
    pub mode: u32,
    pub object: CommitId,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeDelta {
    pub path: RepoPath,
    pub before: Option<TreeEntry>,
    pub after: Option<TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurationEvidence {
    pub schema_version: u32,
    pub source_commit: CommitId,
    pub source_tree: CommitId,
    pub candidate_commit: CommitId,
    pub candidate_tree: CommitId,
    pub source_authorizations: Vec<FileIdentity>,
    pub delta: Vec<TreeDelta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedFile {
    pub(crate) original: FileIdentity,
    pub(crate) source_entry: TreeEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidatePolicy {
    pub(crate) path: RepoPath,
    pub(crate) entry: TreeEntry,
    pub(crate) identity: PolicyIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReleaseManifest {
    pub(crate) schema_version: u32,
    pub(crate) operation_id: OperationId,
    pub(crate) repository_id: String,
    pub(crate) workspace_id: String,
    pub(crate) graph_revision: u64,
    pub(crate) source_commit: CommitId,
    pub(crate) candidate_root: RepoPath,
    pub(crate) baseline: CommitId,
    pub(crate) curation: CurationEvidence,
    pub(crate) policy: CandidatePolicy,
    pub(crate) selected_plans: Vec<PlanIdentity>,
    pub(crate) files: Vec<RetainedFile>,
}
