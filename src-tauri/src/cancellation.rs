//! In-process cancellation registry. Replaces the old Redis-based
//! `task:cancel:{task_id}` flag scheme (G-CANCEL-001) with a single
//! `DashMap<TaskId, Arc<AtomicBool>>` since the new app is a 1-process desktop
//! binary.
//!
//! Phase 2 worker tasks (transcribe / generate / refine) register their task
//! ID at spawn time, then poll the flag at the same checkpoints the old code
//! polled Redis:
//! - before each ASR chunk (G-CANCEL-002)
//! - between batches (G-CANCEL-003)
//! - before token usage recording (G-CANCEL-004)
//!
//! On cancellation the task raises a `Cancelled` error and the wrapper marks
//! the row `status='cancelled'` + posts a timeline event (G-CANCEL-005).

use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub type TaskId = String;

#[derive(Default)]
pub struct Registry {
    flags: DashMap<TaskId, Arc<AtomicBool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new task and return a cloneable cancellation token. The
    /// task should poll `token.load(Ordering::SeqCst)` at every checkpoint.
    pub fn register(&self, task_id: TaskId) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.insert(task_id, flag.clone());
        flag
    }

    /// Signal cancellation. Returns true if the task was registered.
    pub fn cancel(&self, task_id: &str) -> bool {
        if let Some(flag) = self.flags.get(task_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Drop the flag from the registry once the task has finished (success,
    /// failure, or acknowledged cancellation).
    pub fn unregister(&self, task_id: &str) {
        self.flags.remove(task_id);
    }

    /// Check if a task is cancelled without consuming the flag. Mainly for
    /// observability — workers should hold the Arc<AtomicBool> directly.
    pub fn is_cancelled(&self, task_id: &str) -> bool {
        self.flags
            .get(task_id)
            .map(|f| f.load(Ordering::SeqCst))
            .unwrap_or(false)
    }
}
