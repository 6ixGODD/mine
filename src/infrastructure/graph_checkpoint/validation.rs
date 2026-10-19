//! Request-local proof of exact immutable checkpoint inputs. Never persisted,
//! never a Git observation, and never permission to skip successor/CAS checks.
use super::{CheckpointRuntime, GraphBefore, PlanGitEffects};
use crate::domain::{graph::Sha256, lifecycle_operation::*};
use sha2::{Digest, Sha256 as Hasher};

#[derive(Clone)]
pub(crate) struct RuntimeValidationWitness {
    binding: Sha256,
    #[cfg(test)]
    generation: u64,
}

impl RuntimeValidationWitness {
    // Length-frame every exact byte segment; do not serialize/parse the large
    // graph strings again merely to identify the already checked input.
    pub(super) fn binding(
        runtime: &CheckpointRuntime,
        operation: &LifecycleOperation,
    ) -> OperationResult<Sha256> {
        let mut hash = Hasher::new();
        let mut frame = |bytes: &[u8]| {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        };
        frame(b"mine-checkpoint-validation-v1");
        frame(
            &serde_json::to_vec(&(
                &runtime.root,
                &runtime.common_dir,
                &runtime.index_path,
                &runtime.integration_ref,
                &runtime.config_sha256,
                &runtime.source_path,
                &runtime.view_path,
            ))
            .map_err(|_| OperationError::Journal)?,
        );
        match &runtime.before {
            GraphBefore::Present { source, view } => {
                frame(b"present");
                frame(source.as_bytes());
                frame(view.as_bytes());
            }
            GraphBefore::Absent { predecessors } => {
                frame(b"absent");
                frame(&serde_json::to_vec(predecessors).map_err(|_| OperationError::Journal)?);
            }
        }
        frame(runtime.source_after.as_bytes());
        frame(runtime.view_after.as_bytes());
        frame(&runtime.original_index);
        match &runtime.plan_git {
            PlanGitEffects::None => frame(b"none"),
            PlanGitEffects::Withdrawal { intent, .. } => {
                frame(b"withdrawal");
                frame(&serde_json::to_vec(intent).map_err(|_| OperationError::Journal)?);
            }
            PlanGitEffects::Preparation { intent, .. } => {
                frame(b"preparation");
                frame(&serde_json::to_vec(intent).map_err(|_| OperationError::Journal)?);
            }
        }
        let mut identity = operation.fields().clone();
        let checkpoint = identity.checkpoint.take().ok_or(OperationError::Journal)?;
        let checkpoint = checkpoint.fields();
        frame(
            &serde_json::to_vec(&(
                checkpoint.schema_version,
                checkpoint.revision_before,
                &checkpoint.original_head,
                &checkpoint.original_index_sha256,
            ))
            .map_err(|_| OperationError::Journal)?,
        );
        // Only these journal progress fields are excluded. The normal typed
        // stage invariants and successor checks still validate every transition.
        identity.stage_revision = 0;
        identity.stage = OperationStage::GraphCheckpoint(CheckpointStage::Intent);
        identity.outcome = OperationOutcome::Pending;
        frame(&serde_json::to_vec(&identity).map_err(|_| OperationError::Journal)?);
        Ok(Sha256::new(format!("{:x}", hash.finalize()))?)
    }

    pub(super) fn matches(&self, binding: &Sha256) -> bool {
        self.binding == *binding
    }

    // Only runtime::validate_reusing calls this after full or witnessed checks.
    pub(super) fn validated(binding: Sha256) -> Self {
        #[cfg(test)]
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            binding,
            #[cfg(test)]
            generation: GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}
