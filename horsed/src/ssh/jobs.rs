use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::{broadcast, Mutex, RwLock};

#[derive(Clone, Debug)]
pub enum JobEvent {
    Output(Vec<u8>),
    Done(i32),
}

#[derive(Clone)]
pub struct JobRegistry {
    inner: Arc<JobRegistryInner>,
}

struct JobRegistryInner {
    seq: AtomicU64,
    jobs: RwLock<HashMap<String, Arc<JobRecord>>>,
    max_jobs: usize,
    max_bytes_per_job: usize,
}

#[derive(Clone)]
pub struct JobRecord {
    id: String,
    owner: String,
    action: String,
    command: String,
    started_at_ms: u64,
    max_bytes: usize,
    state: Arc<Mutex<JobState>>,
    events: broadcast::Sender<JobEvent>,
}

struct JobState {
    buffer: VecDeque<u8>,
    dropped_bytes: u64,
    finished_at_ms: Option<u64>,
    exit_code: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobSummary {
    pub id: String,
    pub owner: String,
    pub action: String,
    pub command: String,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub running: bool,
    pub dropped_bytes: u64,
    pub subscribers: usize,
}

impl JobRegistry {
    pub fn new(max_jobs: usize, max_bytes_per_job: usize) -> Self {
        Self {
            inner: Arc::new(JobRegistryInner {
                seq: AtomicU64::new(1),
                jobs: RwLock::new(HashMap::new()),
                max_jobs,
                max_bytes_per_job,
            }),
        }
    }

    pub async fn create_job(
        &self,
        owner: impl Into<String>,
        action: impl Into<String>,
        command: impl Into<String>,
    ) -> Arc<JobRecord> {
        let started_at_ms = now_ms();
        let seq = self.inner.seq.fetch_add(1, Ordering::Relaxed);
        let id = format!("job-{started_at_ms:x}-{seq:x}");
        let (events, _rx) = broadcast::channel(512);
        let job = Arc::new(JobRecord {
            id,
            owner: owner.into(),
            action: action.into(),
            command: command.into(),
            started_at_ms,
            max_bytes: self.inner.max_bytes_per_job,
            state: Arc::new(Mutex::new(JobState {
                buffer: VecDeque::new(),
                dropped_bytes: 0,
                finished_at_ms: None,
                exit_code: None,
            })),
            events,
        });

        let mut jobs = self.inner.jobs.write().await;
        jobs.insert(job.id.clone(), job.clone());

        if jobs.len() > self.inner.max_jobs {
            // Prefer removing finished jobs first.
            let mut candidates = jobs
                .iter()
                .filter_map(|(id, job)| {
                    if job.is_finished_sync() {
                        Some((id.clone(), job.started_at_ms))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|(_, started)| *started);
            for (id, _) in candidates {
                if jobs.len() <= self.inner.max_jobs {
                    break;
                }
                jobs.remove(&id);
            }
        }

        job
    }

    pub async fn get_visible(
        &self,
        id: &str,
        user: &str,
        is_admin: bool,
    ) -> Option<Arc<JobRecord>> {
        let jobs = self.inner.jobs.read().await;
        let job = jobs.get(id)?.clone();
        if is_admin || job.owner == user {
            Some(job)
        } else {
            None
        }
    }

    pub async fn list_visible(&self, user: &str, is_admin: bool) -> Vec<JobSummary> {
        let jobs = self.inner.jobs.read().await;
        let mut rows = jobs
            .values()
            .filter(|job| is_admin || job.owner == user)
            .map(|job| job.summary_sync())
            .collect::<Vec<_>>();
        rows.sort_by_key(|it| it.started_at_ms);
        rows.reverse();
        rows
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new(256, 4 * 1024 * 1024)
    }
}

impl JobRecord {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.events.subscribe()
    }

    pub async fn append_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        state.buffer.extend(bytes.iter().copied());
        let overflow = state.buffer.len().saturating_sub(self.max_bytes);
        if overflow > 0 {
            for _ in 0..overflow {
                let _ = state.buffer.pop_front();
            }
            state.dropped_bytes = state.dropped_bytes.saturating_add(overflow as u64);
        }
        let _ = self.events.send(JobEvent::Output(bytes.to_vec()));
    }

    pub async fn finish(&self, exit_code: i32) {
        let mut state = self.state.lock().await;
        if state.exit_code.is_none() {
            state.exit_code = Some(exit_code);
            state.finished_at_ms = Some(now_ms());
            let _ = self.events.send(JobEvent::Done(exit_code));
        }
    }

    pub async fn snapshot(&self) -> (Vec<u8>, Option<i32>, Option<u64>, u64) {
        let state = self.state.lock().await;
        (
            state.buffer.iter().copied().collect(),
            state.exit_code,
            state.finished_at_ms,
            state.dropped_bytes,
        )
    }

    fn is_finished_sync(&self) -> bool {
        if let Ok(state) = self.state.try_lock() {
            state.exit_code.is_some()
        } else {
            false
        }
    }

    fn summary_sync(&self) -> JobSummary {
        let (exit_code, finished_at_ms, dropped_bytes, running) =
            if let Ok(state) = self.state.try_lock() {
                (
                    state.exit_code,
                    state.finished_at_ms,
                    state.dropped_bytes,
                    state.exit_code.is_none(),
                )
            } else {
                (None, None, 0, true)
            };
        JobSummary {
            id: self.id.clone(),
            owner: self.owner.clone(),
            action: self.action.clone(),
            command: self.command.clone(),
            started_at_ms: self.started_at_ms,
            finished_at_ms,
            exit_code,
            running,
            dropped_bytes,
            subscribers: self.events.receiver_count(),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn job_registry_filters_by_owner_and_admin() {
        let jobs = JobRegistry::new(16, 1024);
        let _a = jobs.create_job("alice", "cargo", "build").await;
        let _b = jobs.create_job("bob", "cmd", "ls").await;

        let alice = jobs.list_visible("alice", false).await;
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].owner, "alice");

        let admin = jobs.list_visible("admin", true).await;
        assert_eq!(admin.len(), 2);
    }

    #[tokio::test]
    async fn job_record_keeps_bounded_buffer_and_exit_code() {
        let jobs = JobRegistry::new(16, 4);
        let job = jobs.create_job("alice", "cargo", "test").await;
        job.append_output(b"abcdef").await;

        let (snapshot, exit_code, finished_at_ms, dropped_bytes) = job.snapshot().await;
        assert_eq!(snapshot, b"cdef");
        assert_eq!(dropped_bytes, 2);
        assert_eq!(exit_code, None);
        assert_eq!(finished_at_ms, None);

        job.finish(7).await;
        let (_, exit_code, finished_at_ms, _) = job.snapshot().await;
        assert_eq!(exit_code, Some(7));
        assert!(finished_at_ms.is_some());
    }
}
