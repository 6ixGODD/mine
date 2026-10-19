//! Validated query binding. No journal creation or finalization is advertised.

use crate::domain::error::{ContractResult, contract_text};
mod service;
pub use crate::domain::lifecycle_operation::{
    LifecycleOperation, OperationId, OperationKind, OperationOutcome, OperationStage,
};
pub use service::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationQuery {
    repository_id: String,
    workspace_id: String,
    operation_id: OperationId,
}
impl OperationQuery {
    pub fn new(
        repository_id: String,
        workspace_id: String,
        operation_id: OperationId,
    ) -> ContractResult<Self> {
        contract_text("repository_id", &repository_id)?;
        contract_text("workspace_id", &workspace_id)?;
        Ok(Self {
            repository_id,
            workspace_id,
            operation_id,
        })
    }
    pub fn repository_id(&self) -> &str {
        &self.repository_id
    }
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
}
