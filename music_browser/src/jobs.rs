use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;

// ============================================================================
// Public types
// ============================================================================

/// The kind of entity the caller is targeting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Song,
    LiveSet,
    File,
    Directory,
}

/// Operations the queue can dispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    GenerateSheetMusic,
    Repomix,
    Hitpoints,
}

// ============================================================================
// Configuration (env-overridable)
// ============================================================================

const JOB_STORE_CAP_ENV: &str = "JOB_STORE_CAP";
const JOB_STORE_TTL_SECS_ENV: &str = "JOB_STORE_TTL_SECS";
const HYDRATION_TIMEOUT_SECS_ENV: &str = "HYDRATION_TIMEOUT_SECS";
const HYDRATION_COPY_MAX_BYTES_ENV: &str = "HYDRATION_COPY_MAX_BYTES";

const DEFAULT_JOB_STORE_CAP: usize = 10;
const DEFAULT_JOB_STORE_TTL: Duration = Duration::from_secs(2 * 60 * 60);
const DEFAULT_HYDRATION_TIMEOUT: Duration = Duration::from_secs(30);
// Default limit prevents runaway temp copies but can be disabled with 0.
const DEFAULT_HYDRATION_COPY_MAX_BYTES: Option<u64> = Some(1_024 * 1_024 * 1_024);

#[derive(Debug, Clone)]
struct JobConfig {
    job_store_cap: usize,
    job_store_ttl: Duration,
    hydration_timeout: Duration,
    hydration_copy_max_bytes: Option<u64>,
}

impl JobConfig {
    fn from_env() -> Self {
        Self {
            job_store_cap: parse_usize_env(JOB_STORE_CAP_ENV, DEFAULT_JOB_STORE_CAP),
            job_store_ttl: parse_duration_env(JOB_STORE_TTL_SECS_ENV, DEFAULT_JOB_STORE_TTL),
            hydration_timeout: parse_duration_env(
                HYDRATION_TIMEOUT_SECS_ENV,
                DEFAULT_HYDRATION_TIMEOUT,
            ),
            hydration_copy_max_bytes: parse_opt_u64_env(
                HYDRATION_COPY_MAX_BYTES_ENV,
                DEFAULT_HYDRATION_COPY_MAX_BYTES,
            ),
        }
    }
}

fn config() -> &'static JobConfig {
    static JOB_CONFIG: OnceLock<JobConfig> = OnceLock::new();
    JOB_CONFIG.get_or_init(JobConfig::from_env)
}

fn parse_usize_env(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_duration_env(var: &str, default: Duration) -> Duration {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(default)
}

fn parse_opt_u64_env(var: &str, default: Option<u64>) -> Option<u64> {
    match std::env::var(var) {
        Ok(v) => match v.parse::<u64>() {
            Ok(0) => None,
            Ok(n) => Some(n),
            Err(_) => default,
        },
        Err(_) => default,
    }
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::GenerateSheetMusic => "generate_sheet_music",
            Operation::Repomix => "repomix",
            Operation::Hitpoints => "hitpoints",
        }
    }

    pub fn parse(s: &str) -> Option<Operation> {
        match s {
            "generate_sheet_music" => Some(Operation::GenerateSheetMusic),
            "repomix" => Some(Operation::Repomix),
            "hitpoints" => Some(Operation::Hitpoints),
            _ => None,
        }
    }
}

/// A single unit of work enqueued via `POST /api/workflows`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowJob {
    /// Unique identifier assigned at enqueue time (monotonic u64).
    pub id: u64,
    pub target_type: TargetType,
    /// Song / live-set ID as a string, or an absolute file-system path.
    pub target_id_or_path: String,
    pub operation: Operation,
    /// Resolved absolute path(s) the worker should act on (populated by
    /// entity resolution before the job is sent to the channel).
    ///
    /// Each element is treated as a subprocess *input* — one
    /// `music-operations` invocation is spawned per entry.
    pub resolved_paths: Vec<String>,
    /// Absolute directory that receives any files the operation writes.
    ///
    /// When `None` the worker derives one (e.g. the parent directory of the
    /// input).  Populated by entity resolution for `Song`/`LiveSet` targets
    /// so song output lands in the song's configured `scores_folder` or
    /// `export_folder`.
    #[serde(default)]
    pub output_dir: Option<String>,
}

/// Lightweight handle used to submit jobs from request handlers.
#[derive(Clone)]
pub struct JobQueue {
    sender: mpsc::Sender<WorkflowJob>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    pub store: JobStore,
}

impl JobQueue {
    /// Create a new queue.  Returns the `JobQueue` handle and the receiving
    /// end that the background worker task should own.
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<WorkflowJob>) {
        let (sender, receiver) = mpsc::channel(buffer);
        let queue = JobQueue {
            sender,
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            store: JobStore::new(),
        };
        (queue, receiver)
    }

    /// Assign an ID, register in the store, and submit to the worker.
    /// Returns the assigned job ID.
    pub async fn enqueue(&self, mut job: WorkflowJob) -> Result<u64, EnqueueError> {
        job.id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.store.register(job.clone());
        self.sender
            .send(job.clone())
            .await
            .map_err(|_| EnqueueError::ChannelClosed)?;
        Ok(job.id)
    }
}

/// Errors that can occur when submitting a job.
#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    #[error("job queue channel closed")]
    ChannelClosed,
}

// ============================================================================
// In-memory job store
// ============================================================================

/// Lifecycle state of a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
        }
    }
}

/// A snapshot of a job together with its captured log lines.
#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub job: WorkflowJob,
    pub status: JobStatus,
    pub log_lines: Vec<String>,
    /// Wall-clock instant the job was registered (used for TTL eviction).
    #[serde(skip)]
    pub created_at: std::time::Instant,
}

/// Shared in-memory ring-buffer of recent jobs.
///
/// Old entries are evicted when the configured capacity is exceeded or when an
/// entry is older than the configured TTL. Both `JobQueue` and `run_worker`
/// hold a clone of the `Arc` so they can write into it.
#[derive(Clone)]
pub struct JobStore(Arc<Mutex<VecDeque<JobRecord>>>);

impl JobStore {
    pub fn new() -> Self {
        JobStore(Arc::new(Mutex::new(VecDeque::with_capacity(
            config().job_store_cap + 1,
        ))))
    }

    /// Register a freshly-enqueued job.
    pub fn register(&self, job: WorkflowJob) {
        let mut guard = self.0.lock().unwrap();
        self.evict(&mut guard);
        guard.push_back(JobRecord {
            job,
            status: JobStatus::Queued,
            log_lines: Vec::new(),
            created_at: std::time::Instant::now(),
        });
        if guard.len() > config().job_store_cap {
            guard.pop_front();
        }
    }

    /// Transition a job to `Running`.
    pub fn mark_running(&self, id: u64) {
        self.update(id, |r| r.status = JobStatus::Running);
    }

    /// Append a log line to a job's record.
    pub fn append_log(&self, id: u64, line: String) {
        self.update(id, |r| r.log_lines.push(line));
    }

    /// Transition a job to `Done`.
    pub fn mark_done(&self, id: u64) {
        self.update(id, |r| r.status = JobStatus::Done);
    }

    /// Transition a job to `Failed`.
    pub fn mark_failed(&self, id: u64) {
        self.update(id, |r| r.status = JobStatus::Failed);
    }

    /// Return a snapshot of all live records, newest first.
    pub fn list(&self) -> Vec<JobRecord> {
        let mut guard = self.0.lock().unwrap();
        self.evict(&mut guard);
        guard.iter().cloned().rev().collect()
    }

    /// Return a single record by job ID.
    pub fn get(&self, id: u64) -> Option<JobRecord> {
        let guard = self.0.lock().unwrap();
        guard.iter().find(|r| r.job.id == id).cloned()
    }

    fn update<F: FnOnce(&mut JobRecord)>(&self, id: u64, f: F) {
        let mut guard = self.0.lock().unwrap();
        if let Some(r) = guard.iter_mut().find(|r| r.job.id == id) {
            f(r);
        }
    }

    fn evict(&self, guard: &mut VecDeque<JobRecord>) {
        let now = std::time::Instant::now();
        guard.retain(|r| now.duration_since(r.created_at) < config().job_store_ttl);
    }
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Background worker
// ============================================================================

/// Environment variable used to override the `music-operations` executable.
///
/// Defaults to `music-operations` (picked up from `$PATH`).  Useful when the
/// CLI is installed in a venv that is not on `$PATH`, e.g.
/// `MUSIC_OPERATIONS_BIN=/opt/music-ops/bin/music-operations`.
pub const MUSIC_OPERATIONS_BIN_ENV: &str = "MUSIC_OPERATIONS_BIN";

/// Resolve the `music-operations` executable name.
fn music_operations_bin() -> String {
    std::env::var(MUSIC_OPERATIONS_BIN_ENV).unwrap_or_else(|_| "music-operations".to_string())
}

/// Map a [`Operation`] to the sub-command expected by the `music-operations`
/// CLI (`--operation <X>`).  Returns `None` for operations that the external
/// CLI does not yet support so the caller can surface a clear error.
fn operation_cli_name(op: &Operation) -> Option<&'static str> {
    match op {
        Operation::GenerateSheetMusic => Some("anthemscore"),
        Operation::Repomix => Some("repomix"),
        Operation::Hitpoints => None,
    }
}

/// Consume jobs from `receiver`, hydrate every resolved path, then dispatch
/// the corresponding `music-operations` Python CLI once per input.
///
/// All progress is written into `store` so the `/jobs` UI can display it.
pub async fn run_worker(mut receiver: mpsc::Receiver<WorkflowJob>, store: JobStore) {
    log::info!("workflow job worker started");
    while let Some(job) = receiver.recv().await {
        process_job(&store, job).await;
    }
    log::info!("workflow job worker stopped");
}

macro_rules! job_log {
    ($id:expr, $store:expr, $($arg:tt)*) => {{
        let line = format!($($arg)*);
        log::info!("{}", line);
        $store.append_log($id, line);
    }};
}

async fn process_job(store: &JobStore, job: WorkflowJob) {
    let id = job.id;
    store.mark_running(id);

    job_log!(
        id,
        store,
        "job {id}: started op={} target_type={:?} target={} paths={:?} output_dir={:?}",
        job.operation.as_str(),
        job.target_type,
        job.target_id_or_path,
        job.resolved_paths,
        job.output_dir,
    );

    // Short-circuit operations the external CLI does not support yet.
    let cli_op = match operation_cli_name(&job.operation) {
        Some(name) => name,
        None => {
            job_log!(
                id,
                store,
                "job {id}: operation {:?} is not supported by the music-operations CLI yet",
                job.operation
            );
            store.mark_failed(id);
            return;
        }
    };

    if job.resolved_paths.is_empty() {
        job_log!(id, store, "job {id}: no resolved input paths to dispatch");
        store.mark_failed(id);
        return;
    }

    // Normalise every input path (hydrate OneDrive placeholders / copy to
    // temp dir on failure). Keep the TempDir alive for the duration of the job.
    let mut local_paths: Vec<LocalPathGuard> = Vec::new();
    for raw in &job.resolved_paths {
        let path = PathBuf::from(raw);
        match ensure_local(&path).await {
            Ok(local) => {
                job_log!(
                    id,
                    store,
                    "job {id}: normalized {:?} → {:?}",
                    path,
                    local.path()
                );
                local_paths.push(local);
            }
            Err(e) => {
                job_log!(
                    id,
                    store,
                    "job {id}: could not normalize path {:?}: {e}",
                    path
                );
                store.mark_failed(id);
                return;
            }
        }
    }

    // Dispatch one subprocess per input.  Any non-zero exit fails the job.
    let bin = music_operations_bin();
    for local in &local_paths {
        let input = local.path();

        let output_dir = match resolve_output_dir(&job, input) {
            Ok(d) => d,
            Err(e) => {
                job_log!(id, store, "job {id}: could not resolve output dir: {e}");
                store.mark_failed(id);
                return;
            }
        };

        job_log!(
            id,
            store,
            "job {id}: spawning `{bin} --operation {cli_op} --input-file {} --output-dir {}`",
            input.display(),
            output_dir.display(),
        );

        match spawn_music_operations(id, store, &bin, cli_op, input, &output_dir).await {
            Ok(0) => {
                job_log!(id, store, "job {id}: subprocess exited 0");
            }
            Ok(code) => {
                job_log!(id, store, "job {id}: subprocess exited with code {code}");
                store.mark_failed(id);
                return;
            }
            Err(e) => {
                job_log!(id, store, "job {id}: subprocess spawn failed: {e}");
                store.mark_failed(id);
                return;
            }
        }
    }

    store.mark_done(id);
}

/// Decide the `--output-dir` passed to the subprocess for `input`.
///
/// Uses the job's explicit `output_dir` when present; otherwise falls back to
/// the input's parent directory so generated artefacts land next to the
/// source file.
fn resolve_output_dir(job: &WorkflowJob, input: &Path) -> Result<PathBuf, String> {
    if let Some(explicit) = job.output_dir.as_deref().filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(explicit));
    }
    input
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| format!("input path {:?} has no parent directory", input))
}

/// Run one `music-operations` invocation, tailing stdout/stderr into the
/// job's log.  Returns the subprocess exit code.
async fn spawn_music_operations(
    id: u64,
    store: &JobStore,
    bin: &str,
    cli_op: &str,
    input: &Path,
    output_dir: &Path,
) -> std::io::Result<i32> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    let mut cmd = Command::new(bin);
    cmd.arg("--operation")
        .arg(cli_op)
        .arg("--input-file")
        .arg(input)
        .arg("--output-dir")
        .arg(output_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Forward stdout/stderr line-by-line so UI users get live progress.
    let stdout_task = stdout.map(|out| {
        let store = store.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(out).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                store.append_log(id, format!("[stdout] {line}"));
            }
        })
    });
    let stderr_task = stderr.map(|err| {
        let store = store.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(err).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                store.append_log(id, format!("[stderr] {line}"));
            }
        })
    });

    let status = child.wait().await?;
    if let Some(t) = stdout_task {
        let _ = t.await;
    }
    if let Some(t) = stderr_task {
        let _ = t.await;
    }

    Ok(status.code().unwrap_or(-1))
}

// ============================================================================
// File hydration utility
// ============================================================================

/// Result of a hydration check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HydrationStatus {
    /// File exists and its full contents are present on disk.
    Hydrated,
    /// File entry exists but is a cloud-drive placeholder (sparse / zero-size
    /// on a mounted virtual filesystem such as OneDrive Files On-Demand).
    Placeholder,
    /// No filesystem entry found at this path.
    NotFound,
}

/// Check whether a file at `path` is physically present (fully hydrated) on
/// the local filesystem.
///
/// OneDrive "Files On-Demand" placeholders are regular directory entries whose
/// apparent size is the cloud-side size but whose on-disk allocated blocks are
/// zero.  We detect them with two heuristics that work cross-platform without
/// requiring platform-specific APIs:
///
/// 1. If the path does not exist → `NotFound`.
/// 2. If the path exists but reports size 0 while its extension suggests it
///    should contain data (audio, video, project files) → `Placeholder`.
/// 3. On Unix, if `st_blocks == 0` and `st_size > 0` → `Placeholder`.
/// 4. Otherwise → `Hydrated`.
///
/// This is intentionally conservative: it will never incorrectly mark a
/// locally-present file as a placeholder.
pub fn check_hydration(path: &Path) -> HydrationStatus {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return HydrationStatus::NotFound,
    };

    if !meta.is_file() {
        return HydrationStatus::NotFound;
    }

    let size = meta.len();

    // Platform-specific sparse-file detection (Unix / macOS).
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // st_blocks is measured in 512-byte units; 0 blocks with non-zero size
        // is the hallmark of an OneDrive / iCloud placeholder.
        if meta.blocks() == 0 && size > 0 {
            return HydrationStatus::Placeholder;
        }
    }

    // Heuristic for platforms where st_blocks is unavailable: files whose
    // extension suggests binary content but whose size is 0 are treated as
    // placeholders.
    if size == 0 {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let binary_exts = [
            "wav", "aif", "aiff", "mp3", "flac", "ogg", "m4a", "cpr", "als", "ptx", "ptf", "logic",
            "band", "cubase", "caf", "mid", "midi", "xml", "musicxml",
        ];
        if ext
            .as_deref()
            .map(|e| binary_exts.contains(&e))
            .unwrap_or(false)
        {
            return HydrationStatus::Placeholder;
        }
    }

    HydrationStatus::Hydrated
}

// ============================================================================
// Path normalization
// ============================================================================

/// Guard that keeps a hydrated file path alive along with any tempdir backing it.
#[derive(Debug)]
pub struct LocalPathGuard {
    path: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

impl LocalPathGuard {
    fn owned(path: PathBuf) -> Self {
        Self {
            path,
            _temp_dir: None,
        }
    }

    fn with_tempdir(path: PathBuf, temp_dir: tempfile::TempDir) -> Self {
        Self {
            path,
            _temp_dir: Some(temp_dir),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Errors produced by [`ensure_local`].
#[derive(Debug, thiserror::Error)]
pub enum HydrationError {
    /// The path does not exist at all on the filesystem.
    #[error("path not found: {0}")]
    NotFound(PathBuf),

    /// A platform-specific hydration command was invoked but the file is still
    /// a placeholder after waiting.
    #[error("file is still a placeholder after hydration attempt: {0}")]
    StillPlaceholder(PathBuf),

    /// An I/O error occurred while copying the file to a temp location.
    #[error("failed to copy {src} to temp dir: {source}")]
    CopyFailed {
        src: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Could not create a temporary directory.
    #[error("failed to create temp dir: {0}")]
    TempDirFailed(#[source] std::io::Error),

    /// Copy fallback refused because the file exceeds the configured limit.
    #[error("refused to copy {src} to temp dir: size {size} exceeds limit {limit}")]
    TooLargeForCopy { src: PathBuf, size: u64, limit: u64 },
}

/// Ensure that `path` refers to a file whose bytes are physically present on
/// the local filesystem before it is handed to a Python subprocess.
///
/// ## Behaviour
///
/// | State          | Action                                                        | Returns          |
/// |----------------|---------------------------------------------------------------|------------------|
/// | `Hydrated`     | No-op.                                                        | `Ok(LocalPathGuard { path })` |
/// | `Placeholder`  | Trigger platform hydration, poll until `Hydrated` or timeout, then return the now-local path (or a copy in a temp dir on failure). | `Ok(LocalPathGuard)` |
/// | `NotFound`     | Return an error immediately.                                  | `Err(NotFound)`  |
///
/// The returned guard keeps any fallback `TempDir` alive for the lifetime of
/// the job, avoiding intentional leaks.
pub async fn ensure_local(path: &Path) -> Result<LocalPathGuard, HydrationError> {
    match check_hydration(path) {
        HydrationStatus::Hydrated => return Ok(LocalPathGuard::owned(path.to_path_buf())),
        HydrationStatus::NotFound => return Err(HydrationError::NotFound(path.to_path_buf())),
        HydrationStatus::Placeholder => {}
    }

    log::info!("ensure_local: triggering hydration for {:?}", path);

    trigger_hydration(path).await;

    // Poll until the file is hydrated or we time out.
    let deadline = tokio::time::Instant::now() + config().hydration_timeout;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match check_hydration(path) {
            HydrationStatus::Hydrated => return Ok(LocalPathGuard::owned(path.to_path_buf())),
            HydrationStatus::NotFound => return Err(HydrationError::NotFound(path.to_path_buf())),
            HydrationStatus::Placeholder => {}
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }

    log::warn!(
        "ensure_local: hydration timed out for {:?}, falling back to temp-dir copy",
        path
    );
    copy_to_temp(path, config().hydration_copy_max_bytes).await
}

/// Invoke the platform-native command that requests a cloud file to be
/// downloaded.  This is best-effort: failures are logged but not returned as
/// errors (the caller polls `check_hydration` independently).
async fn trigger_hydration(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        // `attrib +p` sets the "pinned" (always-keep-offline) bit on OneDrive
        // Files On-Demand, which triggers an immediate download.
        let path_str = path.to_string_lossy().into_owned();
        match tokio::process::Command::new("attrib")
            .args(["+p", &path_str])
            .status()
            .await
        {
            Ok(s) => log::debug!("attrib +p exited with {s}"),
            Err(e) => log::warn!("attrib +p failed: {e}"),
        }
    }

    #[cfg(target_os = "macos")]
    {
        // `brctl download` requests iCloud Drive to materialise the file.
        let path_str = path.to_string_lossy().into_owned();
        match tokio::process::Command::new("brctl")
            .args(["download", &path_str])
            .status()
            .await
        {
            Ok(s) => log::debug!("brctl download exited with {s}"),
            Err(e) => log::warn!("brctl download failed: {e}"),
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        log::debug!(
            "trigger_hydration: no platform command available for {:?}",
            path
        );
    }
}

/// Copy `src` into a newly-created temporary directory and return a guard that
/// keeps that directory alive for the caller's lifetime.
async fn copy_to_temp(
    path: &Path,
    max_bytes: Option<u64>,
) -> Result<LocalPathGuard, HydrationError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| HydrationError::NotFound(path.to_path_buf()))?;

    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| HydrationError::CopyFailed {
            src: path.to_path_buf(),
            source: e,
        })?;

    let size = meta.len();
    if let Some(limit) = max_bytes {
        if size > limit {
            return Err(HydrationError::TooLargeForCopy {
                src: path.to_path_buf(),
                size,
                limit,
            });
        }
    }

    let tmp_dir = tempfile::Builder::new()
        .prefix("music_browser_hydration_")
        .tempdir()
        .map_err(HydrationError::TempDirFailed)?;

    let dest = tmp_dir.path().join(file_name);
    tokio::fs::copy(path, &dest)
        .await
        .map_err(|e| HydrationError::CopyFailed {
            src: path.to_path_buf(),
            source: e,
        })?;

    Ok(LocalPathGuard::with_tempdir(dest, tmp_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // --- Operation parse / as_str round-trips ---

    #[test]
    fn test_operation_parse_known_values() {
        let cases = [
            ("generate_sheet_music", Operation::GenerateSheetMusic),
            ("repomix", Operation::Repomix),
            ("hitpoints", Operation::Hitpoints),
        ];
        for (s, expected) in &cases {
            assert_eq!(
                Operation::parse(s),
                Some(expected.clone()),
                "parse failed for {s}"
            );
        }
    }

    #[test]
    fn test_operation_parse_unknown_returns_none() {
        assert_eq!(Operation::parse("unknown_op"), None);
    }

    #[test]
    fn test_operation_as_str_round_trips() {
        for op in &[
            Operation::GenerateSheetMusic,
            Operation::Repomix,
            Operation::Hitpoints,
        ] {
            assert_eq!(
                Operation::parse(op.as_str()),
                Some(op.clone()),
                "round-trip failed for {:?}",
                op
            );
        }
    }

    // --- HydrationStatus ---

    #[test]
    fn test_check_hydration_not_found_for_missing_path() {
        let status = check_hydration(Path::new("/tmp/__nonexistent_music_browser_test__.wav"));
        assert_eq!(
            status,
            HydrationStatus::NotFound,
            "expected NotFound for missing path"
        );
    }

    #[test]
    fn test_check_hydration_hydrated_for_real_file() {
        let mut tmp = NamedTempFile::new().expect("tempfile");
        tmp.write_all(b"RIFF fake wav data")
            .expect("write tempfile");
        let status = check_hydration(tmp.path());
        assert_eq!(
            status,
            HydrationStatus::Hydrated,
            "expected Hydrated for real file at {:?}",
            tmp.path()
        );
    }

    #[test]
    fn test_check_hydration_placeholder_for_zero_size_audio_extension() {
        let tmp = NamedTempFile::new().expect("tempfile");
        let path_with_ext = tmp.path().with_extension("wav");
        std::fs::File::create(&path_with_ext).expect("create empty wav");
        let status = check_hydration(&path_with_ext);
        std::fs::remove_file(&path_with_ext).ok();
        assert_eq!(
            status,
            HydrationStatus::Placeholder,
            "expected Placeholder for zero-size .wav at {:?}",
            path_with_ext
        );
    }

    // --- ensure_local ---

    #[tokio::test]
    async fn test_ensure_local_returns_same_path_for_hydrated_file() {
        let mut tmp = NamedTempFile::new().expect("tempfile");
        tmp.write_all(b"RIFF real audio data").expect("write");
        let result = ensure_local(tmp.path()).await;
        assert!(
            result.is_ok(),
            "expected Ok for hydrated file, got {:?}",
            result
        );
        let guard = result.unwrap();
        assert_eq!(
            guard.path(),
            tmp.path(),
            "expected path to be unchanged for an already-local file"
        );
    }

    #[tokio::test]
    async fn test_ensure_local_returns_not_found_for_missing_path() {
        let missing = Path::new("/tmp/__nonexistent_music_browser_phase2__.wav");
        let result = ensure_local(missing).await;
        assert!(
            matches!(result, Err(HydrationError::NotFound(_))),
            "expected NotFound error for missing path, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_copy_to_temp_produces_readable_file_with_same_name() {
        let mut src = NamedTempFile::new().expect("tempfile");
        src.write_all(b"audio bytes").expect("write");
        let src_path = src.path().to_path_buf();

        let result = copy_to_temp(&src_path, None).await;
        assert!(
            result.is_ok(),
            "expected copy_to_temp to succeed, got {:?}",
            result
        );

        let dest = result.unwrap();
        assert_eq!(
            dest.path().file_name(),
            src_path.file_name(),
            "expected dest filename {:?} to match src {:?}",
            dest.path().file_name(),
            src_path.file_name()
        );

        let content = std::fs::read(dest.path())
            .unwrap_or_else(|e| panic!("could not read copied file {:?}: {e}", dest.path()));
        assert_eq!(
            content, b"audio bytes",
            "copied file content does not match source"
        );
    }

    #[tokio::test]
    async fn test_copy_to_temp_fails_for_nonexistent_source() {
        let missing = Path::new("/tmp/__nonexistent_music_browser_copy_test__.wav");
        let result = copy_to_temp(missing, None).await;
        assert!(
            matches!(result, Err(HydrationError::CopyFailed { .. })),
            "expected CopyFailed for missing source, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_copy_to_temp_respects_max_bytes_limit() {
        let mut src = NamedTempFile::new().expect("tempfile");
        // 8 bytes
        src.write_all(b"12345678").expect("write");
        let src_path = src.path().to_path_buf();

        let result = copy_to_temp(&src_path, Some(4)).await;
        assert!(
            matches!(result, Err(HydrationError::TooLargeForCopy { .. })),
            "expected TooLargeForCopy, got {:?}",
            result
        );
    }

    // --- JobQueue enqueue ---

    #[tokio::test]
    async fn test_job_queue_enqueue_assigns_monotonic_ids() {
        let (queue, mut rx) = JobQueue::new(8);

        let make_job = |path: &str| WorkflowJob {
            id: 0,
            target_type: TargetType::File,
            target_id_or_path: path.to_string(),
            operation: Operation::Hitpoints,
            resolved_paths: vec![path.to_string()],
            output_dir: None,
        };

        let id1 = queue
            .enqueue(make_job("/a/b.wav"))
            .await
            .expect("enqueue 1");
        let id2 = queue
            .enqueue(make_job("/a/c.wav"))
            .await
            .expect("enqueue 2");

        assert!(id1 < id2, "expected id1={id1} < id2={id2}");

        let received1 = rx.recv().await.expect("recv 1");
        let received2 = rx.recv().await.expect("recv 2");

        assert_eq!(
            received1.id, id1,
            "received job id {} != expected {id1}",
            received1.id
        );
        assert_eq!(
            received2.id, id2,
            "received job id {} != expected {id2}",
            received2.id
        );
    }

    // --- Phase 3: subprocess dispatch plumbing ---

    /// Shared lock guarding tests that mutate process-wide env vars, since
    /// `cargo test` runs tests in parallel by default.  Holding this lock
    /// for the duration of such a test prevents the
    /// `MUSIC_OPERATIONS_BIN` value seen by one test from bleeding into
    /// another.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_operation_cli_name_maps_supported_ops() {
        let cases = [
            (Operation::GenerateSheetMusic, Some("anthemscore")),
            (Operation::Repomix, Some("repomix")),
            (Operation::Hitpoints, None),
        ];
        for (op, want) in cases {
            assert_eq!(
                operation_cli_name(&op),
                want,
                "unexpected CLI name for {op:?}: got {:?}, want {want:?}",
                operation_cli_name(&op)
            );
        }
    }

    #[test]
    fn test_music_operations_bin_default_and_override() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        std::env::remove_var(MUSIC_OPERATIONS_BIN_ENV);
        assert_eq!(
            music_operations_bin(),
            "music-operations",
            "expected default when {} is unset",
            MUSIC_OPERATIONS_BIN_ENV
        );

        std::env::set_var(MUSIC_OPERATIONS_BIN_ENV, "/opt/custom/music-ops");
        assert_eq!(
            music_operations_bin(),
            "/opt/custom/music-ops",
            "expected override when {} is set",
            MUSIC_OPERATIONS_BIN_ENV
        );
        std::env::remove_var(MUSIC_OPERATIONS_BIN_ENV);
    }

    #[test]
    fn test_resolve_output_dir_uses_explicit_when_set() {
        let job = WorkflowJob {
            id: 1,
            target_type: TargetType::Song,
            target_id_or_path: "1".into(),
            operation: Operation::GenerateSheetMusic,
            resolved_paths: vec!["/in/song.wav".into()],
            output_dir: Some("/explicit/out".into()),
        };
        let got = resolve_output_dir(&job, Path::new("/in/song.wav"))
            .expect("resolve_output_dir should succeed");
        assert_eq!(
            got,
            PathBuf::from("/explicit/out"),
            "expected explicit output_dir to be used, got {got:?}"
        );
    }

    #[test]
    fn test_resolve_output_dir_falls_back_to_parent() {
        let job = WorkflowJob {
            id: 1,
            target_type: TargetType::File,
            target_id_or_path: "/in/song.wav".into(),
            operation: Operation::Repomix,
            resolved_paths: vec!["/in/song.wav".into()],
            output_dir: None,
        };
        let got = resolve_output_dir(&job, Path::new("/in/song.wav"))
            .expect("resolve_output_dir should succeed");
        assert_eq!(
            got,
            PathBuf::from("/in"),
            "expected parent directory fallback, got {got:?}"
        );
    }

    #[test]
    fn test_resolve_output_dir_treats_empty_string_as_unset() {
        let job = WorkflowJob {
            id: 1,
            target_type: TargetType::File,
            target_id_or_path: "/in/song.wav".into(),
            operation: Operation::Repomix,
            resolved_paths: vec!["/in/song.wav".into()],
            output_dir: Some(String::new()),
        };
        let got = resolve_output_dir(&job, Path::new("/in/song.wav"))
            .expect("resolve_output_dir should succeed");
        assert_eq!(
            got,
            PathBuf::from("/in"),
            "empty string output_dir should fall back to parent, got {got:?}"
        );
    }

    #[test]
    fn test_resolve_output_dir_errors_when_input_has_no_parent() {
        let job = WorkflowJob {
            id: 1,
            target_type: TargetType::File,
            target_id_or_path: "/".into(),
            operation: Operation::Repomix,
            resolved_paths: vec!["/".into()],
            output_dir: None,
        };
        let err =
            resolve_output_dir(&job, Path::new("/")).expect_err("expected Err for rootless input");
        assert!(
            err.contains("no parent"),
            "error message {err:?} should mention missing parent"
        );
    }

    #[tokio::test]
    async fn test_process_job_hitpoints_marks_failed_with_clear_log() {
        let store = JobStore::new();
        let job = WorkflowJob {
            id: 42,
            target_type: TargetType::File,
            target_id_or_path: "/tmp/whatever.wav".into(),
            operation: Operation::Hitpoints,
            resolved_paths: vec!["/tmp/whatever.wav".into()],
            output_dir: None,
        };
        // Pre-register so mark_running/mark_failed affect the record.
        store.register(job.clone());

        process_job(&store, job).await;

        let rec = store
            .get(42)
            .expect("expected job record #42 in store after process_job");
        assert_eq!(
            rec.status,
            JobStatus::Failed,
            "expected Hitpoints job to fail (unsupported by CLI), got {:?} with log={:?}",
            rec.status,
            rec.log_lines
        );
        assert!(
            rec.log_lines
                .iter()
                .any(|l| l.contains("not supported by the music-operations CLI")),
            "expected unsupported-op message in log, got lines={:?}",
            rec.log_lines
        );
    }

    #[tokio::test]
    async fn test_process_job_empty_resolved_paths_fails_fast() {
        let store = JobStore::new();
        let job = WorkflowJob {
            id: 99,
            target_type: TargetType::Song,
            target_id_or_path: "1".into(),
            operation: Operation::Repomix,
            resolved_paths: vec![],
            output_dir: Some("/tmp/out".into()),
        };
        store.register(job.clone());

        process_job(&store, job).await;

        let rec = store.get(99).expect("record #99 missing after process_job");
        assert_eq!(
            rec.status,
            JobStatus::Failed,
            "empty resolved_paths should fail; got {:?} log={:?}",
            rec.status,
            rec.log_lines
        );
        assert!(
            rec.log_lines
                .iter()
                .any(|l| l.contains("no resolved input")),
            "expected no-input diagnostic, got lines={:?}",
            rec.log_lines
        );
    }

    #[tokio::test]
    async fn test_process_job_missing_binary_marks_failed() {
        // Scope the env-var guard so it is released before we await — holding
        // a `std::sync::MutexGuard` across `.await` trips
        // `clippy::await_holding_lock`.  The subprocess we spawn (which
        // doesn't actually exist) reads the env var at spawn time, so the
        // lock is only needed while setting/clearing it.
        {
            let _guard = ENV_LOCK.lock().expect("env lock");
            std::env::set_var(
                MUSIC_OPERATIONS_BIN_ENV,
                "/nonexistent/definitely_not_a_real_binary_xyz",
            );
        }

        let mut src = NamedTempFile::new().expect("tempfile");
        src.write_all(b"fake wav").expect("write");
        let input = src.path().to_path_buf();

        let store = JobStore::new();
        let job = WorkflowJob {
            id: 7,
            target_type: TargetType::File,
            target_id_or_path: input.display().to_string(),
            operation: Operation::Repomix,
            resolved_paths: vec![input.display().to_string()],
            output_dir: None,
        };
        store.register(job.clone());

        process_job(&store, job).await;
        {
            let _guard = ENV_LOCK.lock().expect("env lock");
            std::env::remove_var(MUSIC_OPERATIONS_BIN_ENV);
        }

        let rec = store.get(7).expect("record #7 missing after process_job");
        assert_eq!(
            rec.status,
            JobStatus::Failed,
            "expected failure when binary is missing, got {:?} log={:?}",
            rec.status,
            rec.log_lines
        );
        assert!(
            rec.log_lines
                .iter()
                .any(|l| l.contains("spawn failed") || l.contains("subprocess")),
            "expected spawn-failure diagnostic, got lines={:?}",
            rec.log_lines
        );
    }
}
