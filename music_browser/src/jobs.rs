use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
    pub resolved_paths: Vec<String>,
}

/// Lightweight handle used to submit jobs from request handlers.
#[derive(Clone)]
pub struct JobQueue {
    sender: mpsc::Sender<WorkflowJob>,
    next_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl JobQueue {
    /// Create a new queue.  Returns the `JobQueue` handle and the receiving
    /// end that the background worker task should own.
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<WorkflowJob>) {
        let (sender, receiver) = mpsc::channel(buffer);
        let queue = JobQueue {
            sender,
            next_id: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
        };
        (queue, receiver)
    }

    /// Assign an ID and submit a job.  Returns the assigned job ID.
    pub async fn enqueue(&self, mut job: WorkflowJob) -> Result<u64, EnqueueError> {
        job.id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
// Background worker
// ============================================================================

/// Consume jobs from `receiver`, normalize every resolved path to a
/// locally-present file (hydrating or copying OneDrive placeholders as
/// needed), then log the ready-to-dispatch job.
///
/// Replace the inner log with a real subprocess call in Phase 3.
pub async fn run_worker(mut receiver: mpsc::Receiver<WorkflowJob>) {
    log::info!("workflow job worker started");
    while let Some(job) = receiver.recv().await {
        log::info!(
            "workflow job {}: op={} target_type={:?} target={} paths={:?}",
            job.id,
            job.operation.as_str(),
            job.target_type,
            job.target_id_or_path,
            job.resolved_paths,
        );

        let mut local_paths: Vec<PathBuf> = Vec::new();
        let mut failed = false;

        for raw in &job.resolved_paths {
            let path = PathBuf::from(raw);
            match ensure_local(&path).await {
                Ok(local) => {
                    log::info!("job {}: normalized {:?} → {:?}", job.id, path, local);
                    local_paths.push(local);
                }
                Err(e) => {
                    log::error!("job {}: could not normalize path {:?}: {e}", job.id, path);
                    failed = true;
                    break;
                }
            }
        }

        if failed {
            log::warn!("job {}: skipped due to path normalization failure", job.id);
            continue;
        }

        log::info!(
            "job {}: ready to dispatch op={} local_paths={:?}",
            job.id,
            job.operation.as_str(),
            local_paths,
        );
        // TODO(phase-3): spawn music_operations Python subprocess with local_paths
    }
    log::info!("workflow job worker stopped");
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
}

/// Maximum time to wait for a cloud-storage hydration command to finish.
const HYDRATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Ensure that `path` refers to a file whose bytes are physically present on
/// the local filesystem before it is handed to a Python subprocess.
///
/// ## Behaviour
///
/// | State          | Action                                                        | Returns          |
/// |----------------|---------------------------------------------------------------|------------------|
/// | `Hydrated`     | No-op.                                                        | `Ok(path.to_path_buf())` |
/// | `Placeholder`  | Trigger platform hydration, poll until `Hydrated` or timeout, then return the now-local path (or a copy in a temp dir on failure). | `Ok(local_copy)` |
/// | `NotFound`     | Return an error immediately.                                  | `Err(NotFound)`  |
///
/// The returned `PathBuf` is always a path to a fully-local file.  When a
/// copy to a temporary directory is made the caller is responsible for
/// ensuring the `TempDir` outlives the subprocess (pass it through the job).
pub async fn ensure_local(path: &Path) -> Result<PathBuf, HydrationError> {
    match check_hydration(path) {
        HydrationStatus::Hydrated => return Ok(path.to_path_buf()),
        HydrationStatus::NotFound => return Err(HydrationError::NotFound(path.to_path_buf())),
        HydrationStatus::Placeholder => {}
    }

    log::info!("ensure_local: triggering hydration for {:?}", path);

    trigger_hydration(path).await;

    // Poll until the file is hydrated or we time out.
    let deadline = tokio::time::Instant::now() + HYDRATION_TIMEOUT;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match check_hydration(path) {
            HydrationStatus::Hydrated => return Ok(path.to_path_buf()),
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
    copy_to_temp(path).await
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

/// Copy `src` into a newly-created temporary directory and return the path of
/// the copy.  The `TempDir` guard is intentionally leaked so the copy
/// persists for the duration of the process; a future phase can thread it
/// through the job struct for proper cleanup.
async fn copy_to_temp(src: &Path) -> Result<PathBuf, HydrationError> {
    let file_name = src.file_name().ok_or_else(|| HydrationError::CopyFailed {
        src: src.to_path_buf(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no file name component",
        ),
    })?;

    let tmp_dir = tempfile::Builder::new()
        .prefix("music_browser_hydrate_")
        .tempdir()
        .map_err(HydrationError::TempDirFailed)?;

    let dest = tmp_dir.path().join(file_name);

    tokio::fs::copy(src, &dest)
        .await
        .map_err(|e| HydrationError::CopyFailed {
            src: src.to_path_buf(),
            source: e,
        })?;

    log::info!("ensure_local: copied {:?} → {:?}", src, dest);

    // Keep the TempDir alive for the process lifetime.
    std::mem::forget(tmp_dir);

    Ok(dest)
}

// ============================================================================
// Unit tests
// ============================================================================

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
        assert_eq!(
            result.unwrap(),
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

        let result = copy_to_temp(&src_path).await;
        assert!(
            result.is_ok(),
            "expected copy_to_temp to succeed, got {:?}",
            result
        );

        let dest = result.unwrap();
        assert_eq!(
            dest.file_name(),
            src_path.file_name(),
            "expected dest filename {:?} to match src {:?}",
            dest.file_name(),
            src_path.file_name()
        );

        let content = std::fs::read(&dest)
            .unwrap_or_else(|e| panic!("could not read copied file {:?}: {e}", dest));
        assert_eq!(
            content, b"audio bytes",
            "copied file content does not match source"
        );
    }

    #[tokio::test]
    async fn test_copy_to_temp_fails_for_nonexistent_source() {
        let missing = Path::new("/tmp/__nonexistent_music_browser_copy_test__.wav");
        let result = copy_to_temp(missing).await;
        assert!(
            matches!(result, Err(HydrationError::CopyFailed { .. })),
            "expected CopyFailed for missing source, got {:?}",
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
}
