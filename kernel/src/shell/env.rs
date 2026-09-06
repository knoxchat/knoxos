/// Shell environment variables and command history management
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use spin::Mutex;

/// Shell environment variables
lazy_static::lazy_static! {
    pub static ref ENV_VARS: Mutex<BTreeMap<String, String>> = {
        let mut env = BTreeMap::new();
        env.insert(String::from("HOME"), String::from("/home/user"));
        env.insert(String::from("USER"), String::from("user"));
        env.insert(String::from("SHELL"), String::from("/bin/ksh"));
        env.insert(String::from("PATH"), String::from("/bin:/usr/bin:/sbin:/usr/sbin"));
        env.insert(String::from("PWD"), String::from("/home/user"));
        env.insert(String::from("TERM"), String::from("linux"));
        env.insert(String::from("HOSTNAME"), String::from("knoxos"));
        env.insert(String::from("LANG"), String::from("en_US.UTF-8"));
        env.insert(String::from("PS1"), String::from("\\u@\\h:\\w$ "));
        env.insert(String::from("PS2"), String::from("> "));
        env.insert(String::from("EDITOR"), String::from("vi"));
        Mutex::new(env)
    };
}

/// Command history ring buffer
lazy_static::lazy_static! {
    pub static ref HISTORY: Mutex<Vec<String>> = Mutex::new(Vec::new());

    /// Shell array variables: name -> Vec<String>
    pub static ref SHELL_ARRAYS: Mutex<BTreeMap<String, Vec<String>>> =
        Mutex::new(BTreeMap::new());

    /// Shell aliases: alias_name -> expansion_string
    pub static ref SHELL_ALIASES: Mutex<BTreeMap<String, String>> =
        Mutex::new(BTreeMap::new());
}

/// Last exit code ($?) — atomic for lock-free access
pub static LAST_EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Last background PID ($!) — atomic for lock-free access
pub static LAST_BG_PID: AtomicU32 = AtomicU32::new(0);

/// Background job tracking
lazy_static::lazy_static! {
    pub static ref BACKGROUND_JOBS: Mutex<Vec<BackgroundJob>> = Mutex::new(Vec::new());
}

/// A background job entry
#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub job_id: usize,
    pub pid: u32,
    pub command: String,
    pub status: JobStatus,
}

/// Background job status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobStatus {
    Running,
    Stopped,
    Done,
}

/// Get the last exit code
pub fn get_last_exit_code() -> i32 {
    LAST_EXIT_CODE.load(Ordering::Relaxed)
}

/// Set the last exit code
pub fn set_last_exit_code(code: i32) {
    LAST_EXIT_CODE.store(code, Ordering::Relaxed);
}

/// Get the shell PID (from scheduler or fallback to 1)
pub fn get_shell_pid() -> u32 {
    crate::scheduler::current_pid().unwrap_or(1)
}

/// Add a background job, returns the job ID
pub fn add_background_job(pid: u32, command: &str) -> usize {
    let mut jobs = BACKGROUND_JOBS.lock();
    let job_id = jobs.len() + 1;
    jobs.push(BackgroundJob {
        job_id,
        pid,
        command: String::from(command),
        status: JobStatus::Running,
    });
    LAST_BG_PID.store(pid, Ordering::Relaxed);
    job_id
}

/// Stop the current foreground job (called by SIGTSTP / Ctrl+Z handler)
/// Moves the foreground process to the job table as stopped
pub fn stop_foreground_job() {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    if pid == 0 || pid == 1 {
        return; // Don't stop the shell itself
    }
    let mut jobs = BACKGROUND_JOBS.lock();
    // Check if this PID is already in the job table
    if let Some(job) = jobs.iter_mut().find(|j| j.pid == pid) {
        job.status = JobStatus::Stopped;
    } else {
        // Add as a new stopped job
        let job_id = jobs.len() + 1;
        jobs.push(BackgroundJob {
            job_id,
            pid,
            command: String::from("(stopped)"),
            status: JobStatus::Stopped,
        });
    }
}

// ── Array variable operations ───────────────────────────────────

/// Set an array variable: arr=(a b c)
pub fn set_array(name: &str, values: Vec<String>) {
    SHELL_ARRAYS.lock().insert(String::from(name), values);
}

/// Get array element: ${arr[N]}
pub fn get_array_element(name: &str, index: usize) -> Option<String> {
    SHELL_ARRAYS.lock().get(name)?.get(index).cloned()
}

/// Get all array elements: ${arr[@]} or ${arr[*]}
pub fn get_array_all(name: &str) -> Option<Vec<String>> {
    SHELL_ARRAYS.lock().get(name).cloned()
}

/// Get array length: ${#arr[@]}
pub fn get_array_length(name: &str) -> usize {
    SHELL_ARRAYS.lock().get(name).map(|a| a.len()).unwrap_or(0)
}

/// Set array element: arr[N]=value
pub fn set_array_element(name: &str, index: usize, value: String) {
    let mut arrays = SHELL_ARRAYS.lock();
    let arr = arrays.entry(String::from(name)).or_default();
    while arr.len() <= index {
        arr.push(String::new());
    }
    arr[index] = value;
}

/// Maximum command history entries
const MAX_HISTORY: usize = 500;

/// Add a command to history (deduplicates consecutive entries)
pub fn add_history(cmd: &str) {
    if cmd.is_empty() {
        return;
    }
    let mut history = HISTORY.lock();
    // Don't add duplicates of the last command
    if history.last().map(|s| s.as_str()) == Some(cmd) {
        return;
    }
    history.push(String::from(cmd));
    if history.len() > MAX_HISTORY {
        history.remove(0);
    }
}

/// Get a clone of the full history
pub fn get_history() -> Vec<String> {
    HISTORY.lock().clone()
}

/// Get the number of history entries
pub fn history_len() -> usize {
    HISTORY.lock().len()
}

/// Clear all history
pub fn clear_history() {
    HISTORY.lock().clear();
}

/// Get an environment variable value
pub fn get_var(key: &str) -> Option<String> {
    ENV_VARS.lock().get(key).cloned()
}

/// Set an environment variable
pub fn set_var(key: &str, value: &str) {
    ENV_VARS
        .lock()
        .insert(String::from(key), String::from(value));
}

/// Remove an environment variable
pub fn unset_var(key: &str) {
    ENV_VARS.lock().remove(key);
}

/// Get PWD with fallback
pub fn pwd() -> String {
    ENV_VARS
        .lock()
        .get("PWD")
        .cloned()
        .unwrap_or_else(|| String::from("/"))
}
