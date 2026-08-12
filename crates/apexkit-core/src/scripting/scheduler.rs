use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Global Quantum Round-Robin Scheduler singleton
static SCHEDULER: OnceLock<QuantumScheduler> = OnceLock::new();

pub fn get_quantum_scheduler() -> &'static QuantumScheduler {
    SCHEDULER.get_or_init(QuantumScheduler::new)
}

pub struct TaskControl {
    pub running: Mutex<bool>,
    pub condvar: Condvar,
}

struct TaskHandle {
    control: Arc<TaskControl>,
    accumulated_cpu: Duration,
    max_cpu_budget: Duration,
    quantum_slice: Duration,
    last_slice_start: Option<Instant>,
}

struct SchedulerState {
    queue: VecDeque<u64>,
    active_task_id: Option<u64>,
    tasks: HashMap<u64, TaskHandle>,
    next_id: u64,
}

pub struct QuantumScheduler {
    state: Mutex<SchedulerState>,
}

impl QuantumScheduler {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SchedulerState {
                queue: VecDeque::new(),
                active_task_id: None,
                tasks: HashMap::new(),
                next_id: 1,
            }),
        }
    }

    /// Registers a new script task with its total budget (e.g., 300ms) and quantum slice (10ms)
    pub fn register_task(&self, max_cpu_budget: Duration, quantum_slice: Duration) -> u64 {
        let mut state = self.state.lock().unwrap();
        let task_id = state.next_id;
        state.next_id += 1;

        let control = Arc::new(TaskControl {
            running: Mutex::new(false),
            condvar: Condvar::new(),
        });

        let handle = TaskHandle {
            control,
            accumulated_cpu: Duration::ZERO,
            max_cpu_budget,
            quantum_slice,
            last_slice_start: None,
        };

        state.tasks.insert(task_id, handle);
        state.queue.push_back(task_id);

        if state.active_task_id.is_none() {
            state.active_task_id = Some(task_id);
            if let Some(t) = state.tasks.get_mut(&task_id) {
                t.last_slice_start = Some(Instant::now());
                let mut running = t.control.running.lock().unwrap();
                *running = true;
            }
        }

        task_id
    }

    /// Called by the QuickJS interrupt handler periodically.
    /// Returns `true` if the process must ABORT (budget exhausted).
    pub fn check_and_yield(&self, task_id: u64) -> bool {
        let mut state = self.state.lock().unwrap();

        let (elapsed_slice, total_cpu, max_budget, quantum_slice, control) = {
            let Some(task) = state.tasks.get_mut(&task_id) else {
                return true; // Task un-registered, stop execution
            };

            let now = Instant::now();
            let slice_time = task
                .last_slice_start
                .map(|start| now.duration_since(start))
                .unwrap_or(Duration::ZERO);

            let total = task.accumulated_cpu + slice_time;
            (
                slice_time,
                total,
                task.max_cpu_budget,
                task.quantum_slice,
                task.control.clone(),
            )
        };

        // 1. Hard Cap Check: Has the script exhausted its total CPU allowance (e.g. 300ms)?
        if total_cpu >= max_budget {
            tracing::warn!(
                "🛑 [Scheduler] Task {} exhausted total CPU budget ({:?}/{:?}). Terminating.",
                task_id,
                total_cpu,
                max_budget
            );
            return true; // Interrupt QuickJS -> Abort execution
        }

        // 2. Quantum Slice Check: Has it run continuously for 10ms?
        if elapsed_slice >= quantum_slice {
            if let Some(task) = state.tasks.get_mut(&task_id) {
                task.accumulated_cpu += elapsed_slice;
                task.last_slice_start = None;
            }

            state.queue.retain(|&id| id != task_id);
            state.queue.push_back(task_id);

            self.rotate_next_task(&mut state);

            if state.active_task_id == Some(task_id) {
                if let Some(task) = state.tasks.get_mut(&task_id) {
                    task.last_slice_start = Some(Instant::now());
                }
                return false;
            }

            drop(state); // Release scheduler mutex while parked

            let mut running = control.running.lock().unwrap();
            while !*running {
                running = control.condvar.wait(running).unwrap();
            }
            *running = false; // Reset flag for next rotation

            let mut state = self.state.lock().unwrap();
            if let Some(task) = state.tasks.get_mut(&task_id) {
                task.last_slice_start = Some(Instant::now());
            }

            return false;
        }

        false
    }

    /// Called when a script yields for async I/O (e.g. fetch, $db.query).
    /// Pauses CPU timer and gives the CPU slot to another script immediately.
    pub fn pause_for_io(&self, task_id: u64) {
        let mut state = self.state.lock().unwrap();
        if let Some(task) = state.tasks.get_mut(&task_id) {
            if let Some(start) = task.last_slice_start {
                task.accumulated_cpu += start.elapsed();
                task.last_slice_start = None;
            }
        }
        state.queue.retain(|&id| id != task_id);
        if state.active_task_id == Some(task_id) {
            self.rotate_next_task(&mut state);
        }
    }

    /// Called when I/O completes and script is ready to run JS bytecodes again.
    pub fn resume_from_io(&self, task_id: u64) {
        let mut state = self.state.lock().unwrap();
        if !state.queue.contains(&task_id) {
            state.queue.push_back(task_id);
        }

        if state.active_task_id.is_none() {
            self.rotate_next_task(&mut state);
        } else if state.active_task_id != Some(task_id) {
            let control = state.tasks.get(&task_id).map(|t| t.control.clone());
            drop(state);

            if let Some(control) = control {
                let mut running = control.running.lock().unwrap();
                while !*running {
                    running = control.condvar.wait(running).unwrap();
                }
                *running = false;
            }

            let mut state = self.state.lock().unwrap();
            if let Some(task) = state.tasks.get_mut(&task_id) {
                task.last_slice_start = Some(Instant::now());
            }
        }
    }

    pub fn unregister_task(&self, task_id: u64) {
        let mut state = self.state.lock().unwrap();
        state.tasks.remove(&task_id);
        state.queue.retain(|&id| id != task_id);

        if state.active_task_id == Some(task_id) {
            self.rotate_next_task(&mut state);
        }
    }

    fn rotate_next_task(&self, state: &mut SchedulerState) {
        state.active_task_id = None;
        if let Some(next_id) = state.queue.front().copied() {
            state.active_task_id = Some(next_id);
            if let Some(next_task) = state.tasks.get_mut(&next_id) {
                next_task.last_slice_start = Some(Instant::now());
                let mut running = next_task.control.running.lock().unwrap();
                *running = true;
                next_task.control.condvar.notify_one();
            }
        }
    }
}

pub struct QuantumGuard {
    task_id: u64,
}

impl QuantumGuard {
    pub fn new(task_id: u64) -> Self {
        Self { task_id }
    }
    pub fn id(&self) -> u64 {
        self.task_id
    }
}

impl Drop for QuantumGuard {
    fn drop(&mut self) {
        get_quantum_scheduler().unregister_task(self.task_id);
    }
}
