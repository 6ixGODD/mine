use super::VerificationLocation;
use crate::application::verification_service::VerificationBackend;
use crate::domain::graph::{FileIdentity, PlanIdentity, RepoPath, Sha256};
use crate::domain::verification_receipt::*;
use crate::infrastructure::graph_snapshot::resolve_contained;
use sha2::{Digest, Sha256 as Hasher};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_JSON: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const OUTPUTS: &[&str] = &[
    ".git",
    ".mine/build",
    ".mine/runtime",
    ".mine/locks",
    ".mine/worktrees",
];

fn refused(message: &'static str) -> VerificationError {
    VerificationError::Refused(message)
}
fn path(value: &str) -> VerificationResult<RepoPath> {
    Ok(RepoPath::new(value)?)
}
fn reserved(value: &RepoPath, verification_root: &str) -> bool {
    #[cfg(windows)]
    let normalized = value.as_str().to_ascii_lowercase();
    #[cfg(not(windows))]
    let normalized = value.as_str();
    OUTPUTS
        .iter()
        .copied()
        .chain(std::iter::once(verification_root))
        .any(|root| normalized == root || normalized.starts_with(&format!("{root}/")))
}
// Check lexical spelling first, then the existing platform-canonical locator.
// Neither path performs file-content IO; Windows case/short-name aliases cannot
// turn an explicitly reserved lock into a source dependency.
fn reserved_at(
    root: &Path,
    relative: &RepoPath,
    verification_root: &str,
) -> VerificationResult<bool> {
    if reserved(relative, verification_root) {
        return Ok(true);
    }
    let resolved = safe_path(root, relative)?;
    let canonical_relative = resolved
        .strip_prefix(root)
        .map_err(|_| refused("input outside source root"))?;
    let value = canonical_relative
        .to_str()
        .ok_or(refused("non-UTF-8 input path"))?
        .replace('\\', "/");
    Ok(reserved(&path(&value)?, verification_root))
}
fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn mode(metadata: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o7777
    }
    #[cfg(not(unix))]
    {
        u32::from(metadata.permissions().readonly())
    }
}

/// Symlinks/junctions are conservatively refused, even contained ones. This
/// also refuses cycles before traversal and avoids silently hashing only the
/// referent while losing the link's own identity. Concurrent hostile filesystem
/// replacement is outside the local trusted-workspace boundary (no unsafe IO).
fn safe_path(root: &Path, relative: &RepoPath) -> VerificationResult<PathBuf> {
    let mut candidate = root.to_path_buf();
    for part in relative.as_str().split('/') {
        let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
        if part.ends_with(['.', ' '])
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(refused("nonportable or device input path"));
        }
        candidate.push(part);
        match fs::symlink_metadata(&candidate) {
            Ok(meta) if is_link(&meta) => {
                return Err(refused("links and reparse points are not inputs"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    resolve_contained(root, relative)
        .map_err(|_| refused("path containment could not be established"))
}
fn read_bounded(file: &Path, limit: u64) -> VerificationResult<Vec<u8>> {
    if !fs::metadata(file)?.is_file() {
        return Err(refused("expected regular file"));
    }
    let mut bytes = Vec::new();
    File::open(file)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(refused("evidence exceeds retention limit"));
    }
    Ok(bytes)
}
fn identity(root: &Path, relative: &RepoPath) -> VerificationResult<FileIdentity> {
    let target = safe_path(root, relative)?;
    let before = fs::metadata(&target)?;
    if !before.is_file() {
        return Err(refused("expected regular input file"));
    }
    let mut reader = File::open(&target)?;
    let mut hasher = Hasher::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        hasher.update(&buffer[..read]);
    }
    let after = fs::metadata(safe_path(root, relative)?)?;
    if bytes != before.len()
        || bytes != after.len()
        || mode(&before) != mode(&after)
        || before.modified()? != after.modified()?
    {
        return Err(refused("input changed while being observed"));
    }
    Ok(FileIdentity {
        path: relative.clone(),
        bytes,
        sha256: Sha256::new(format!("{:x}", hasher.finalize())).expect("SHA-256"),
    })
}

#[derive(Debug, Clone)]
pub struct FileVerificationStore {
    root: PathBuf,
    output_root: PathBuf,
    verification_root: String,
}
impl FileVerificationStore {
    /// The caller resolves the assigned source checkout; Root lifecycle authority
    /// is supplied separately to VerificationService. No Git child is launched.
    pub fn new(source_root: &Path, authoritative_root: &Path) -> VerificationResult<Self> {
        let output_root = authoritative_root.canonicalize()?;
        let config_path = output_root.join(".mine/config.toml");
        let layout = if config_path.try_exists()? {
            let config = crate::domain::config::MineConfig::parse(
                &config_path,
                &fs::read_to_string(&config_path)?,
            )
            .map_err(|_| refused("invalid Root governance configuration"))?;
            config
                .layout()
                .map_err(|_| refused("invalid Root governance layout"))?
        } else {
            crate::domain::config::GovernanceLayout::canonical()
        };
        Self::for_bound_layout(source_root, &output_root, &layout)
    }

    /// Internal retained-release adapter: its validated source and candidate
    /// snapshots bind different layouts. Historical receipt locators must not
    /// be reinterpreted using the mutable, post-integration Root configuration.
    pub(crate) fn for_bound_layout(
        source_root: &Path,
        authoritative_root: &Path,
        layout: &crate::domain::config::GovernanceLayout,
    ) -> VerificationResult<Self> {
        let root = source_root.canonicalize()?;
        let output_root = authoritative_root.canonicalize()?;
        if !root.is_dir() || !output_root.is_dir() {
            return Err(refused("source/Root is not a directory"));
        }
        let verification_root = format!("{}/reports/verification", layout.plan_dir().as_str());
        Ok(Self {
            root,
            output_root,
            verification_root,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn load_policy(&self, policy_path: &RepoPath) -> VerificationResult<VerificationPolicy> {
        Ok(serde_json::from_slice(&read_bounded(
            &safe_path(&self.root, policy_path)?,
            MAX_JSON,
        )?)?)
    }
    fn directory(&self, plan: &PlanIdentity) -> VerificationResult<RepoPath> {
        let directory = plan.verification_directory();
        if directory.as_str() != format!("{}/{}", self.verification_root, plan.fields().plan_id) {
            return Err(refused("Plan evidence differs from configured Root layout"));
        }
        Ok(directory)
    }
    fn receipt_path(&self, plan: &PlanIdentity, id: &ReceiptId) -> VerificationResult<RepoPath> {
        path(&format!(
            "{}/{}.json",
            self.directory(plan)?.as_str(),
            id.as_str()
        ))
    }
    fn lock(&self, plan: &PlanIdentity) -> VerificationResult<File> {
        let location = path(&format!(
            ".mine/runtime/verification-locks/{}.lock",
            plan.fields().plan_id
        ))?;
        let target = safe_path(&self.output_root, &location)?;
        fs::create_dir_all(target.parent().expect("lock parent"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(safe_path(&self.output_root, &location)?)?;
        fs4::FileExt::try_lock(&file).map_err(|_| refused("verification store is active"))?;
        Ok(file)
    }
    fn publish(&self, location: &RepoPath, bytes: &[u8]) -> VerificationResult<()> {
        let target = safe_path(&self.root, location)?;
        let parent = target.parent().expect("evidence parent");
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{}.tmp", ReceiptId::generate().as_str()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            let target = safe_path(&self.root, location)?;
            if target.exists() {
                return Err(refused("append-only evidence already exists"));
            }
            fs::rename(&temporary, &target)?;
            Ok(())
        })();
        if result.is_err() && temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        result
    }

    fn retention_capacity(
        &self,
        plan: &PlanIdentity,
        additional_bytes: u64,
    ) -> VerificationResult<()> {
        let directory = safe_path(&self.root, &self.directory(plan)?)?;
        let mut files = 0usize;
        let mut bytes = additional_bytes;
        if directory.exists() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| refused("invalid evidence name"))?;
                let relative = path(&format!("{}/{name}", self.directory(plan)?.as_str()))?;
                let metadata = fs::metadata(safe_path(&self.root, &relative)?)?;
                if !metadata.is_file() {
                    return Err(refused("unexpected evidence directory entry"));
                }
                files += 1;
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or(refused("retention length overflow"))?;
            }
        }
        if files >= 128 || bytes > 32 * 1024 * 1024 {
            return Err(refused("Plan evidence retention capacity exceeded"));
        }
        Ok(())
    }

    fn walk(
        &self,
        relative: Option<&RepoPath>,
        entries: &mut BTreeMap<RepoPath, InputEntry>,
        depth: usize,
    ) -> VerificationResult<()> {
        if depth > 128 || entries.len() >= MAX_ENTRIES {
            return Err(refused("input traversal limit exceeded"));
        }
        if let Some(relative) = relative {
            if reserved_at(&self.root, relative, &self.verification_root)? {
                return Ok(());
            }
        }
        let target = match relative {
            Some(p) => safe_path(&self.root, p)?,
            None => self.root.clone(),
        };
        let metadata = fs::symlink_metadata(&target)?;
        if is_link(&metadata) {
            return Err(refused("linked input refused"));
        }
        let entries_before = entries.len();
        if let Some(p) = relative.filter(|_| !metadata.is_dir()) {
            let file = if metadata.is_dir() {
                FileIdentity {
                    path: p.clone(),
                    bytes: 0,
                    sha256: sha256(b"directory"),
                }
            } else {
                identity(&self.root, p)?
            };
            entries.insert(
                p.clone(),
                InputEntry {
                    file,
                    directory: metadata.is_dir(),
                    mode: mode(&metadata),
                },
            );
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&target)? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| refused("non-UTF-8 input path"))?;
                let child = path(&match relative {
                    Some(p) => format!("{}/{name}", p.as_str()),
                    None => name,
                })?;
                self.walk(Some(&child), entries, depth + 1)?;
            }
            if let Some(p) = relative {
                // Creating an excluded output must not introduce its otherwise
                // empty ancestor directories into the next input key.
                let output_ancestor = OUTPUTS
                    .iter()
                    .copied()
                    .chain(std::iter::once(self.verification_root.as_str()))
                    .any(|o| {
                        #[cfg(windows)]
                        let parent = p.as_str().to_ascii_lowercase();
                        #[cfg(not(windows))]
                        let parent = p.as_str();
                        o.starts_with(&format!("{parent}/"))
                    });
                if entries.len() > entries_before || !output_ancestor {
                    entries.insert(
                        p.clone(),
                        InputEntry {
                            file: FileIdentity {
                                path: p.clone(),
                                bytes: 0,
                                sha256: sha256(b"directory"),
                            },
                            directory: true,
                            mode: mode(&metadata),
                        },
                    );
                }
            }
        } else if !metadata.is_file() {
            return Err(refused("special input files are unsupported"));
        }
        Ok(())
    }

    fn validate_scope(&self, selected: &CheckPolicy) -> VerificationResult<()> {
        for lock in &selected.dependency_locks {
            if reserved_at(&self.root, lock, &self.verification_root)? {
                return Err(refused("lockfile cannot be a disposable output"));
            }
        }
        if let CommandDirectory::Relative(cwd) = &selected.command.fields().cwd {
            if reserved_at(&self.root, cwd, &self.verification_root)? {
                return Err(refused("command working directory cannot be an output"));
            }
        }
        if let InputScope::DependencyClosure { declaration: d } = &selected.inputs {
            for input in d
                .roots
                .iter()
                .chain(&d.generator_inputs)
                .chain(std::iter::once(&d.audit.path))
            {
                if reserved_at(&self.root, input, &self.verification_root)? {
                    return Err(refused("dependency scope cannot name an output"));
                }
            }
            if identity(&self.root, &d.audit.path)? != d.audit {
                return Err(refused("dependency audit identity mismatch"));
            }
            let audit: DependencyAudit = serde_json::from_slice(&read_bounded(
                &safe_path(&self.root, &d.audit.path)?,
                MAX_JSON,
            )?)?;
            if audit.schema_version != 1
                || audit.command != selected.command
                || audit.roots != d.roots
                || audit.generator_inputs != d.generator_inputs
                || audit.excluded != d.excluded
                || audit.reviewed_by.trim().is_empty()
                || audit.rationale.trim().is_empty()
            {
                return Err(refused(
                    "dependency audit does not prove this exact command scope",
                ));
            }
        }
        Ok(())
    }
}

impl VerificationBackend for FileVerificationStore {
    fn manifest(
        &self,
        policy: &VerificationPolicy,
        check: &str,
    ) -> VerificationResult<InputManifest> {
        let selected = policy.check(check)?;
        self.validate_scope(selected)?;
        if let CommandDirectory::Relative(cwd) = &selected.command.fields().cwd {
            if !safe_path(&self.root, cwd)?.is_dir()
                || reserved_at(&self.root, cwd, &self.verification_root)?
            {
                return Err(refused(
                    "command working directory is not a source directory",
                ));
            }
        }
        let mut entries = BTreeMap::new();
        match &selected.inputs {
            InputScope::Full => self.walk(None, &mut entries, 0)?,
            InputScope::DependencyClosure { declaration: d } => {
                for input in d
                    .roots
                    .iter()
                    .chain(&d.generator_inputs)
                    .chain(std::iter::once(&d.audit.path))
                {
                    self.walk(Some(input), &mut entries, 0)?;
                }
            }
        }
        for lock in &selected.dependency_locks {
            if reserved_at(&self.root, lock, &self.verification_root)? {
                return Err(refused("lockfile cannot be a disposable output"));
            }
            self.walk(Some(lock), &mut entries, 0)?;
        }
        Ok(InputManifest {
            schema_version: 1,
            policy_sha256: policy.identity().fields().sha256.clone(),
            command: selected.command.clone(),
            root_mode: mode(&fs::metadata(&self.root)?),
            working_directory_mode: mode(&fs::metadata(match &selected.command.fields().cwd {
                CommandDirectory::Root => self.root.clone(),
                CommandDirectory::Relative(relative) => safe_path(&self.root, relative)?,
            })?),
            entries: entries.into_values().collect(),
        })
    }
    fn file(&self, relative: &RepoPath) -> VerificationResult<FileIdentity> {
        identity(&self.root, relative)
    }
    fn plan_body(&self, plan: &PlanIdentity) -> VerificationResult<FileIdentity> {
        // Ordinary execution still requires the real immutable Plan in its
        // assigned source checkout. No archive/Root fallback is introduced.
        self.file(&plan.fields().plan_path)
    }
    fn artifacts(
        &self,
        receipt: &VerificationReceipt,
        policy: &CheckPolicy,
    ) -> VerificationResult<()> {
        let fields = receipt.fields();
        if fields.artifacts.is_empty() || fields.artifacts.len() > policy.retention_files {
            return Err(refused("bounded decisive evidence is required"));
        }
        let location = VerificationLocation::new(&self.root, &fields.plan, &fields.receipt_id)
            .map_err(|_| refused("invalid receipt location"))?;
        let mut total = 0u64;
        for artifact in &fields.artifacts {
            location
                .artifact(&artifact.path)
                .map_err(|_| refused("artifact outside exact Plan evidence directory"))?;
            total = total
                .checked_add(artifact.bytes)
                .ok_or(refused("artifact length overflow"))?;
            if total > policy.retention_bytes || artifact.bytes == 0 {
                return Err(refused(
                    "evidence retention limit exceeded or empty decisive artifact",
                ));
            }
            if fs::metadata(safe_path(&self.root, &artifact.path)?)?.len() != artifact.bytes
                || identity(&self.root, &artifact.path)? != *artifact
            {
                return Err(refused("artifact bytes or digest mismatch"));
            }
        }
        Ok(())
    }
    fn load(
        &self,
        plan: &PlanIdentity,
        id: &ReceiptId,
    ) -> VerificationResult<Option<StoredVerification>> {
        let target = safe_path(&self.root, &self.receipt_path(plan, id)?)?;
        match fs::symlink_metadata(&target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
            Ok(_) => Ok(Some(
                crate::infrastructure::strict_json::strict_json(&read_bounded(&target, MAX_JSON)?)
                    .map_err(|_| refused("malformed or duplicate-field stored verification"))?,
            )),
        }
    }
    fn save(&self, evidence: &StoredVerification) -> VerificationResult<()> {
        evidence.validate()?;
        let fields = evidence.receipt.fields();
        let _lock = self.lock(&fields.plan)?;
        self.artifacts(&evidence.receipt, evidence.policy.check(&evidence.check)?)?;
        let bytes = serde_json::to_vec(evidence)?;
        if bytes.len() as u64 > MAX_JSON {
            return Err(refused("receipt exceeds storage limit"));
        }
        self.retention_capacity(&fields.plan, bytes.len() as u64)?;
        self.publish(
            &self.receipt_path(&fields.plan, &fields.receipt_id)?,
            &bytes,
        )
    }
}

/// Held for the complete lifetime of an output user. All cooperating producers
/// must acquire this lease; a dead PID is never used to infer release. An OS
/// lock, not a stale marker or an advisory PID inventory, gates cleanup.
pub struct OutputLease {
    _lock: File,
}
impl FileVerificationStore {
    fn output_path(plan: &PlanIdentity) -> VerificationResult<RepoPath> {
        path(&format!(".mine/build/{}", plan.fields().plan_id))
    }
    fn output_lock(&self, plan: &PlanIdentity, exclusive: bool) -> VerificationResult<File> {
        let relative = path(&format!(
            ".mine/runtime/verification-output-locks/{}.lock",
            plan.fields().plan_id
        ))?;
        let target = safe_path(&self.output_root, &relative)?;
        fs::create_dir_all(target.parent().expect("lock parent"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(safe_path(&self.output_root, &relative)?)?;
        let acquired = if exclusive {
            fs4::FileExt::try_lock(&file)
        } else {
            fs4::FileExt::try_lock_shared(&file)
        };
        acquired.map_err(|_| refused("output has an active user"))?;
        Ok(file)
    }
    pub fn lease_outputs(&self, plan: &PlanIdentity) -> VerificationResult<OutputLease> {
        Ok(OutputLease {
            _lock: self.output_lock(plan, false)?,
        })
    }
    /// Copy one exact owned output into bounded, append-only portable evidence.
    /// Caller holds the producer lease until all record operations complete.
    pub fn retain(
        &self,
        plan: &PlanIdentity,
        source: &RepoPath,
        policy: &CheckPolicy,
    ) -> VerificationResult<FileIdentity> {
        if !within(&Self::output_path(plan)?, source) || source == &Self::output_path(plan)? {
            return Err(refused("retention source is not an exact Plan output file"));
        }
        let _lease = self.lease_outputs(plan)?;
        let _lock = self.lock(plan)?;
        let bytes = read_bounded(
            &safe_path(&self.output_root, source)?,
            policy.retention_bytes,
        )?;
        if bytes.is_empty() {
            return Err(refused("decisive evidence must not be empty"));
        }
        self.retention_capacity(plan, bytes.len() as u64)?;
        let target = path(&format!(
            "{}/{}.log",
            self.directory(plan)?.as_str(),
            ReceiptId::generate().as_str()
        ))?;
        self.publish(&target, &bytes)?;
        identity(&self.root, &target)
    }
    /// Root/integration owner invokes on the evidence-preserving checkout.
    /// Never deletes a worktree, branch, receipt or foreign Plan output. All
    /// stored receipts for this Plan must have retained intact decisive evidence.
    pub fn cleanup_outputs(&self, plan: &PlanIdentity) -> VerificationResult<bool> {
        let _output_lock = self.output_lock(plan, true)?;
        let _store_lock = self.lock(plan)?;
        let directory = safe_path(&self.root, &self.directory(plan)?)?;
        let mut receipts = 0;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| refused("invalid evidence name"))?;
            let relative = path(&format!("{}/{name}", self.directory(plan)?.as_str()))?;
            if name.starts_with('.') || !safe_path(&self.root, &relative)?.is_file() {
                return Err(refused(
                    "unfinished or unsupported evidence prevents cleanup",
                ));
            }
            if entry.path().extension().is_some_and(|e| e == "json") {
                let id = ReceiptId::parse(name.trim_end_matches(".json"))?;
                let stored = self
                    .load(plan, &id)?
                    .ok_or(refused("receipt disappeared before cleanup"))?;
                stored.validate()?;
                if stored.schema_version != 1
                    || stored.receipt.fields().receipt_id != id
                    || stored.receipt.fields().plan != *plan
                    || stored.receipt.fields().input_manifest_sha256 != stored.manifest.digest()
                    || stored.receipt.fields().policy != stored.policy.identity()
                {
                    return Err(refused("unverifiable receipt prevents cleanup"));
                }
                self.artifacts(&stored.receipt, stored.policy.check(&stored.check)?)?;
                receipts += 1;
            }
        }
        if receipts == 0 {
            return Err(refused("no retained verification evidence"));
        }
        let output = Self::output_path(plan)?;
        let target = safe_path(&self.output_root, &output)?;
        if !target.exists() {
            return Ok(false);
        }
        validate_cleanup_tree(&self.output_root, &output, 0)?;
        let fresh = safe_path(&self.output_root, &output)?;
        if fresh != target || target == self.output_root {
            return Err(refused("cleanup target changed"));
        }
        fs::remove_dir_all(&target)?;
        Ok(true)
    }
}
fn validate_cleanup_tree(root: &Path, relative: &RepoPath, depth: usize) -> VerificationResult<()> {
    if depth > 128 {
        return Err(refused("cleanup traversal limit"));
    }
    let target = safe_path(root, relative)?;
    if target.is_dir() {
        for entry in fs::read_dir(target)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| refused("invalid cleanup path"))?;
            validate_cleanup_tree(
                root,
                &path(&format!("{}/{name}", relative.as_str()))?,
                depth + 1,
            )?;
        }
    }
    Ok(())
}
