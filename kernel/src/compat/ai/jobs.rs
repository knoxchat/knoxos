use alloc::string::String;
use alloc::vec::Vec;

/// Inference job
#[derive(Debug, Clone)]
pub struct InferenceJob {
    pub job_id: u64,
    pub model_name: String,
    pub status: InferenceJobStatus,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

lazy_static::lazy_static! {
    static ref INFERENCE_QUEUE: spin::Mutex<Vec<InferenceJob>> = spin::Mutex::new(Vec::new());
    static ref NEXT_JOB_ID: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);
}

/// Submit an inference job
pub fn submit_inference_job(model_name: &str, _input: &str) -> u64 {
    let id = NEXT_JOB_ID.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
    INFERENCE_QUEUE.lock().push(InferenceJob {
        job_id: id,
        model_name: String::from(model_name),
        status: InferenceJobStatus::Queued,
        result: None,
    });
    id
}

/// Poll inference job status
pub fn poll_inference_job(job_id: u64) -> Option<InferenceJob> {
    INFERENCE_QUEUE
        .lock()
        .iter()
        .find(|j| j.job_id == job_id)
        .cloned()
}

/// Get count of running/queued jobs
pub fn active_inference_jobs() -> (usize, usize) {
    let queue = INFERENCE_QUEUE.lock();
    let running = queue
        .iter()
        .filter(|j| j.status == InferenceJobStatus::Running)
        .count();
    let queued = queue
        .iter()
        .filter(|j| j.status == InferenceJobStatus::Queued)
        .count();
    (running, queued)
}
