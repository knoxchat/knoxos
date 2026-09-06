/// Kernel Work Queue — Deferred work / bottom-half processing
///
/// Implements Linux-style work queues for scheduling deferred work:
///   - Work items can be queued from interrupt context
///   - Processed in a safe non-interrupt context
///   - Supports delayed work (timer-based)
///   - Multiple named work queues (like Linux kworker threads)
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

/// Work function type — takes no arguments
pub type WorkFn = fn();

/// A work item
#[derive(Clone)]
pub struct WorkItem {
    /// Unique ID
    pub id: u64,
    /// Name for debugging
    pub name: String,
    /// The function to execute
    pub func: WorkFn,
    /// Execute after this tick count (0 = immediate)
    pub delay_until: u64,
    /// Whether this is a recurring work item
    pub recurring: bool,
    /// Interval for recurring items (in ticks)
    pub interval: u64,
}

/// A named work queue
struct WorkQueue {
    name: String,
    items: VecDeque<WorkItem>,
    processed: u64,
}

/// Global work queue state
struct WorkQueueSystem {
    queues: Vec<WorkQueue>,
    next_id: u64,
    tick_count: u64,
}

lazy_static::lazy_static! {
    static ref WORK_SYSTEM: Mutex<WorkQueueSystem> = Mutex::new(WorkQueueSystem {
        queues: Vec::new(),
        next_id: 1,
        tick_count: 0,
    });
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Create a named work queue
pub fn create_workqueue(name: &str) -> usize {
    let mut sys = WORK_SYSTEM.lock();
    let idx = sys.queues.len();
    sys.queues.push(WorkQueue {
        name: String::from(name),
        items: VecDeque::new(),
        processed: 0,
    });
    serial_println!("[workqueue] Created work queue '{}' (idx={})", name, idx);
    idx
}

/// Queue work to the default work queue
pub fn queue_work(name: &str, func: WorkFn) -> u64 {
    queue_work_on(0, name, func)
}

/// Queue work to a specific work queue
pub fn queue_work_on(queue_idx: usize, name: &str, func: WorkFn) -> u64 {
    let mut sys = WORK_SYSTEM.lock();

    if queue_idx >= sys.queues.len() {
        return 0;
    }

    let id = sys.next_id;
    sys.next_id += 1;

    let item = WorkItem {
        id,
        name: String::from(name),
        func,
        delay_until: 0,
        recurring: false,
        interval: 0,
    };

    sys.queues[queue_idx].items.push_back(item);
    id
}

/// Queue delayed work (execute after `delay_ticks`)
pub fn queue_delayed_work(queue_idx: usize, name: &str, func: WorkFn, delay_ticks: u64) -> u64 {
    let mut sys = WORK_SYSTEM.lock();

    if queue_idx >= sys.queues.len() {
        return 0;
    }

    let id = sys.next_id;
    sys.next_id += 1;

    let delay_until = sys.tick_count + delay_ticks;

    let item = WorkItem {
        id,
        name: String::from(name),
        func,
        delay_until,
        recurring: false,
        interval: 0,
    };

    sys.queues[queue_idx].items.push_back(item);
    id
}

/// Queue recurring work
pub fn queue_recurring_work(
    queue_idx: usize,
    name: &str,
    func: WorkFn,
    interval_ticks: u64,
) -> u64 {
    let mut sys = WORK_SYSTEM.lock();

    if queue_idx >= sys.queues.len() {
        return 0;
    }

    let id = sys.next_id;
    sys.next_id += 1;

    let item = WorkItem {
        id,
        name: String::from(name),
        func,
        delay_until: sys.tick_count + interval_ticks,
        recurring: true,
        interval: interval_ticks,
    };

    sys.queues[queue_idx].items.push_back(item);
    id
}

/// Cancel a work item by ID
pub fn cancel_work(id: u64) -> bool {
    let mut sys = WORK_SYSTEM.lock();
    for queue in &mut sys.queues {
        if let Some(pos) = queue.items.iter().position(|w| w.id == id) {
            queue.items.remove(pos);
            return true;
        }
    }
    false
}

/// Process pending work items on all queues (called from scheduler/idle)
pub fn process_work() {
    if !INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let mut to_execute: Vec<WorkItem> = Vec::new();
    let mut to_requeue: Vec<(usize, WorkItem)> = Vec::new();

    {
        let mut sys = WORK_SYSTEM.lock();
        sys.tick_count += 1;
        let current_tick = sys.tick_count;

        for (queue_idx, queue) in sys.queues.iter_mut().enumerate() {
            let mut remaining = VecDeque::new();

            while let Some(item) = queue.items.pop_front() {
                if item.delay_until <= current_tick {
                    // Ready to execute
                    if item.recurring {
                        let mut requeued = item.clone();
                        requeued.delay_until = current_tick + requeued.interval;
                        to_requeue.push((queue_idx, requeued));
                    }
                    to_execute.push(item);
                } else {
                    remaining.push_back(item);
                }
            }

            queue.items = remaining;
        }

        // Re-add recurring items
        for (idx, item) in to_requeue {
            if idx < sys.queues.len() {
                sys.queues[idx].items.push_back(item);
            }
        }
    }

    // Execute work items outside the lock
    for item in to_execute {
        (item.func)();
        let mut sys = WORK_SYSTEM.lock();
        if let Some(queue) = sys.queues.first_mut() {
            queue.processed += 1;
        }
    }
}

/// Get statistics for all work queues
pub fn stats() -> Vec<(String, usize, u64)> {
    let sys = WORK_SYSTEM.lock();
    sys.queues
        .iter()
        .map(|q| (q.name.clone(), q.items.len(), q.processed))
        .collect()
}

/// Initialize the work queue subsystem
pub fn init() {
    // Create default work queues
    create_workqueue("events"); // General purpose
    create_workqueue("events_highpri"); // High priority
    create_workqueue("events_long"); // Long-running work
    create_workqueue("kblockd"); // Block device work

    INITIALIZED.store(true, Ordering::Release);
    serial_println!("[KnoxOS] Work queue subsystem initialized (4 queues)");
}
