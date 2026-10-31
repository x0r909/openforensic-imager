use std::collections::BTreeMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use forensic_core::{CancellationToken, JobId, UtcTimestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Partial,
    Failed,
    Cancelled,
}

impl JobStatus {
    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Partial | Self::Failed | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSnapshot {
    pub id: JobId,
    pub kind: String,
    pub status: JobStatus,
    pub created_at: UtcTimestamp,
    pub started_at: Option<UtcTimestamp>,
    pub completed_at: Option<UtcTimestamp>,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub cancellation_requested: bool,
    pub message: Option<String>,
    pub error_code: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobOutcome {
    partial: bool,
    result: Value,
    message: Option<String>,
}

impl JobOutcome {
    pub fn succeeded(result: Value) -> Self {
        Self {
            partial: false,
            result,
            message: None,
        }
    }

    pub fn partial(result: Value, message: impl Into<String>) -> Self {
        Self {
            partial: true,
            result,
            message: Some(message.into()),
        }
    }

    pub fn is_partial(&self) -> bool {
        self.partial
    }

    pub fn result(&self) -> &Value {
        &self.result
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    kind: JobErrorKind,
    code: String,
    message: String,
    result: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobErrorKind {
    Failed,
    Cancelled,
}

impl JobError {
    pub fn failed(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: JobErrorKind::Failed,
            code: code.into(),
            message: message.into(),
            result: None,
        }
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self {
            kind: JobErrorKind::Cancelled,
            code: "cancelled".to_owned(),
            message: message.into(),
            result: None,
        }
    }

    /// A cancelled job that still has retained output worth reporting.
    ///
    /// ACQ-011 requires a cancelled acquisition to retain its partial artifacts,
    /// so the staged path travels with the cancellation instead of being
    /// discarded when the job status becomes `cancelled`.
    pub fn cancelled_with_result(result: Value, message: impl Into<String>) -> Self {
        Self {
            kind: JobErrorKind::Cancelled,
            code: "cancelled".to_owned(),
            message: message.into(),
            result: Some(result),
        }
    }

    /// Returns the payload that survived a cancellation, if any.
    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for JobError {}

#[derive(Clone)]
pub struct JobContext {
    id: JobId,
    registry: Arc<JobRegistry>,
    cancellation: CancellationToken,
}

impl JobContext {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn update_progress(&self, completed_bytes: u64, total_bytes: u64) -> Result<(), JobError> {
        if completed_bytes > total_bytes {
            return Err(JobError::failed(
                "job.invalid_progress",
                "completed progress cannot exceed the declared total",
            ));
        }
        let mut jobs = self.registry.jobs.lock().map_err(lock_error)?;
        let slot = jobs
            .get_mut(&self.id)
            .ok_or_else(|| JobError::failed("job.not_found", "job no longer exists"))?;
        slot.snapshot.completed_bytes = completed_bytes;
        slot.snapshot.total_bytes = total_bytes;
        Ok(())
    }

    pub fn set_message(&self, message: impl Into<String>) -> Result<(), JobError> {
        let mut jobs = self.registry.jobs.lock().map_err(lock_error)?;
        let slot = jobs
            .get_mut(&self.id)
            .ok_or_else(|| JobError::failed("job.not_found", "job no longer exists"))?;
        slot.snapshot.message = Some(message.into());
        Ok(())
    }
}

#[derive(Clone)]
pub struct JobManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    runtime: Runtime,
    registry: Arc<JobRegistry>,
}

struct JobRegistry {
    jobs: Mutex<BTreeMap<JobId, JobSlot>>,
    finished: Condvar,
}

struct JobSlot {
    snapshot: JobSnapshot,
    cancellation: CancellationToken,
}

impl JobManager {
    pub fn new(worker_threads: usize) -> Result<Self, JobError> {
        if worker_threads == 0 {
            return Err(JobError::failed(
                "job.invalid_worker_count",
                "worker thread count must be at least one",
            ));
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(worker_threads)
            .enable_time()
            .thread_name("ofi-job")
            .build()
            .map_err(|error| JobError::failed("job.runtime", error.to_string()))?;
        Ok(Self {
            inner: Arc::new(ManagerInner {
                runtime,
                registry: Arc::new(JobRegistry {
                    jobs: Mutex::new(BTreeMap::new()),
                    finished: Condvar::new(),
                }),
            }),
        })
    }

    pub fn submit<F>(&self, kind: impl Into<String>, work: F) -> Result<JobId, JobError>
    where
        F: FnOnce(JobContext) -> Result<JobOutcome, JobError> + Send + 'static,
    {
        let id = JobId::new_v4();
        let snapshot = JobSnapshot {
            id,
            kind: kind.into(),
            status: JobStatus::Pending,
            created_at: UtcTimestamp::now(),
            started_at: None,
            completed_at: None,
            completed_bytes: 0,
            total_bytes: 0,
            cancellation_requested: false,
            message: None,
            error_code: None,
            result: None,
        };
        let cancellation = CancellationToken::default();
        {
            let mut registry = self.inner.registry.jobs.lock().map_err(lock_error)?;
            registry.insert(
                id,
                JobSlot {
                    snapshot,
                    cancellation: cancellation.clone(),
                },
            );
        }
        let registry = Arc::clone(&self.inner.registry);
        self.inner.runtime.spawn_blocking(move || {
            run_job(registry, id, cancellation, work);
        });
        Ok(id)
    }

    pub fn snapshot(&self, id: &JobId) -> Result<Option<JobSnapshot>, JobError> {
        let registry = self.inner.registry.jobs.lock().map_err(lock_error)?;
        Ok(registry.get(id).map(|slot| slot.snapshot.clone()))
    }

    pub fn cancel(&self, id: &JobId) -> bool {
        let Ok(mut registry) = self.inner.registry.jobs.lock() else {
            return false;
        };
        let Some(slot) = registry.get_mut(id) else {
            return false;
        };
        if slot.snapshot.status.is_terminal() {
            return false;
        }
        slot.cancellation.cancel();
        slot.snapshot.cancellation_requested = true;
        true
    }

    pub fn wait(&self, id: &JobId, timeout: Duration) -> Result<Option<JobSnapshot>, JobError> {
        let deadline = Instant::now() + timeout;
        let mut jobs = self.inner.registry.jobs.lock().map_err(lock_error)?;
        loop {
            let Some(slot) = jobs.get(id) else {
                return Ok(None);
            };
            if slot.snapshot.status.is_terminal() {
                return Ok(Some(slot.snapshot.clone()));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(JobError::failed(
                    "job.timeout",
                    "timed out waiting for job completion",
                ));
            }
            let (next, result) = self
                .inner
                .registry
                .finished
                .wait_timeout(jobs, deadline - now)
                .map_err(lock_error)?;
            jobs = next;
            if result.timed_out() {
                return Err(JobError::failed(
                    "job.timeout",
                    "timed out waiting for job completion",
                ));
            }
        }
    }
}

fn run_job<F>(registry: Arc<JobRegistry>, id: JobId, cancellation: CancellationToken, work: F)
where
    F: FnOnce(JobContext) -> Result<JobOutcome, JobError>,
{
    if let Ok(mut jobs) = registry.jobs.lock()
        && let Some(slot) = jobs.get_mut(&id)
    {
        slot.snapshot.status = JobStatus::Running;
        slot.snapshot.started_at = Some(UtcTimestamp::now());
    }
    let context = JobContext {
        id,
        registry: Arc::clone(&registry),
        cancellation: cancellation.clone(),
    };
    let outcome = catch_unwind(AssertUnwindSafe(|| work(context)));
    let result = match outcome {
        Ok(Ok(outcome)) => {
            let status = if outcome.partial {
                JobStatus::Partial
            } else {
                JobStatus::Succeeded
            };
            (status, outcome.message, None, Some(outcome.result))
        }
        Ok(Err(error)) => {
            let status = if error.kind == JobErrorKind::Cancelled {
                JobStatus::Cancelled
            } else {
                JobStatus::Failed
            };
            // A cancelled job may still carry retained artifacts (ACQ-011), so
            // the payload is preserved rather than dropped with the error.
            (status, Some(error.message), Some(error.code), error.result)
        }
        Err(_) => (
            JobStatus::Failed,
            Some("worker panic was contained".to_owned()),
            Some("panic".to_owned()),
            None,
        ),
    };
    if let Ok(mut jobs) = registry.jobs.lock() {
        if let Some(slot) = jobs.get_mut(&id) {
            slot.snapshot.status = result.0;
            slot.snapshot.completed_at = Some(UtcTimestamp::now());
            slot.snapshot.message = result.1;
            slot.snapshot.error_code = result.2;
            slot.snapshot.result = result.3;
            slot.snapshot.cancellation_requested = cancellation.is_cancelled();
        }
        registry.finished.notify_all();
    }
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> JobError {
    JobError::failed("job.lock", "job registry lock was poisoned")
}

impl fmt::Debug for JobManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("JobManager").finish_non_exhaustive()
    }
}
