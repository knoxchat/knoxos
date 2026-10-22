// SPDX-License-Identifier: MIT
//! Single-threaded foreground dispatcher (GPUI `PlatformDispatcher` role).
//!
//! No threads, no tokio. [`dispatch_after`] is a tick deadline against a clock
//! the host or kernel advances. The kernel feeds TSC / PIT ticks; host tests
//! use [`PlatformDispatcher::advance_clock`].

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

type Job = Box<dyn FnOnce() + Send>;

struct Delayed {
    at: u64,
    job: Job,
}

/// Foreground queue + delayed tasks. UI-thread only.
pub struct PlatformDispatcher {
    foreground: VecDeque<Job>,
    delayed: Vec<Delayed>,
    clock: u64,
}

impl PlatformDispatcher {
    pub fn new() -> Self {
        Self {
            foreground: VecDeque::new(),
            delayed: Vec::new(),
            clock: 0,
        }
    }

    pub fn now(&self) -> u64 {
        self.clock
    }

    /// Replace the clock (kernel ticks / TSC snapshot).
    pub fn set_now(&mut self, ticks: u64) {
        self.clock = ticks;
    }

    /// Host fake clock: `now += ticks`.
    pub fn advance_clock(&mut self, ticks: u64) {
        self.clock = self.clock.saturating_add(ticks);
    }

    pub fn dispatch(&mut self, job: Job) {
        self.foreground.push_back(job);
    }

    pub fn dispatch_after(&mut self, delay_ticks: u64, job: Job) {
        self.delayed.push(Delayed {
            at: self.clock.saturating_add(delay_ticks),
            job,
        });
    }

    pub fn has_work(&self) -> bool {
        !self.foreground.is_empty() || self.delayed.iter().any(|d| d.at <= self.clock)
    }

    /// Run due delayed jobs, then drain the foreground queue.
    /// A job that `dispatch_after`s for `now` waits until the next pump.
    pub fn pump(&mut self) {
        let now = self.clock;
        let mut i = 0;
        while i < self.delayed.len() {
            if self.delayed[i].at <= now {
                let Delayed { job, .. } = self.delayed.remove(i);
                self.foreground.push_back(job);
            } else {
                i += 1;
            }
        }
        while let Some(job) = self.foreground.pop_front() {
            job();
        }
    }
}

impl Default for PlatformDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::PlatformDispatcher;
    use alloc::boxed::Box;
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    /// D2.3 done-when: a delayed closure runs on the next pump in a host fake clock test.
    #[test]
    fn delayed_runs_on_next_pump_after_clock() {
        let mut d = PlatformDispatcher::new();
        let ran = Arc::new(AtomicBool::new(false));
        let flag = ran.clone();
        d.dispatch_after(
            10,
            Box::new(move || {
                flag.store(true, Ordering::SeqCst);
            }),
        );

        d.pump();
        assert!(!ran.load(Ordering::SeqCst), "must not run before the delay");
        assert!(!d.has_work());

        d.advance_clock(9);
        d.pump();
        assert!(!ran.load(Ordering::SeqCst));

        d.advance_clock(1);
        assert!(d.has_work());
        d.pump();
        assert!(ran.load(Ordering::SeqCst));
        assert!(!d.has_work());
    }

    #[test]
    fn foreground_runs_same_pump() {
        let mut d = PlatformDispatcher::new();
        let n = Arc::new(AtomicU32::new(0));
        let a = n.clone();
        let b = n.clone();
        d.dispatch(Box::new(move || {
            a.fetch_add(1, Ordering::SeqCst);
        }));
        d.dispatch(Box::new(move || {
            b.fetch_add(10, Ordering::SeqCst);
        }));
        d.pump();
        assert_eq!(n.load(Ordering::SeqCst), 11);
    }
}
