//! Concurrent task scheduling example
//!
//! This example demonstrates using work-stealing deques to build a simple
//! task scheduler where worker threads can steal tasks from each other,
//! providing efficient load balancing for parallel workloads.

use velocityx::deque::WorkStealingDeque;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Task {
    id: u64,
    workload: usize, // Simulated work units
    result: Option<f64>,
}

impl Task {
    fn new(id: u64, workload: usize) -> Self {
        Self {
            id,
            workload,
            result: None,
        }
    }

    fn execute(&mut self) {
        // Simulate computational work
        let mut result = 0.0;
        for i in 0..self.workload {
            result += (i as f64).sin().sqrt();
            if i % 1000 == 0 {
                // Occasionally yield to simulate I/O or other blocking operations
                thread::yield_now();
            }
        }
        self.result = Some(result);
    }
}

struct Worker {
    id: usize,
    deque: Arc<WorkStealingDeque<Task>>,
    steal_targets: Vec<Arc<WorkStealingDeque<Task>>>,
    stats: WorkerStats,
}

#[derive(Debug, Default)]
struct WorkerStats {
    tasks_executed: usize,
    tasks_stolen: usize,
    local_tasks: usize,
    steal_attempts: usize,
    total_work_units: usize,
}

impl Worker {
    fn new(
        id: usize,
        deque: Arc<WorkStealingDeque<Task>>,
        steal_targets: Vec<Arc<WorkStealingDeque<Task>>>,
    ) -> Self {
        Self {
            id,
            deque,
            steal_targets,
            stats: WorkerStats::default(),
        }
    }

    fn run(&mut self, barrier: Arc<Barrier>) -> WorkerStats {
        // Wait for all workers to be ready
        barrier.wait();

        loop {
            // Try to get work from local deque first
            if let Some(mut task) = self.deque.pop() {
                self.stats.local_tasks += 1;
                self.stats.total_work_units += task.workload;
                task.execute();
                self.stats.tasks_executed += 1;
                
                // Occasionally check for steal opportunities
                if self.stats.tasks_executed % 10 == 0 && self.deque.len() < 2 {
                    self.try_steal();
                }
            } else {
                // No local work, try to steal
                if !self.try_steal() {
                    // No work available anywhere, check if we're done
                    if self.all_deques_empty() {
                        break;
                    }
                    thread::yield_now();
                }
            }
        }

        self.stats.clone()
    }

    fn try_steal(&mut self) -> bool {
        self.stats.steal_attempts += 1;

        // Try to steal from random targets to avoid contention
        let mut indices: Vec<usize> = (0..self.steal_targets.len()).collect();
        // Simple shuffle using Fisher-Yates
        for i in (1..indices.len()).rev() {
            let j = (self.stats.steal_attempts + i) % indices.len();
            indices.swap(i, j);
        }

        for &target_idx in &indices {
            if let Some(mut task) = self.steal_targets[target_idx].steal() {
                self.stats.tasks_stolen += 1;
                self.stats.total_work_units += task.workload;
                task.execute();
                self.stats.tasks_executed += 1;
                return true;
            }
        }

        false
    }

    fn all_deques_empty(&self) -> bool {
        // Check if all deques (including our own) are empty
        if !self.deque.is_empty() {
            return false;
        }

        for target in &self.steal_targets {
            if !target.is_empty() {
                return false;
            }
        }

        true
    }
}

fn main() {
    println!("⚡ Concurrent Task Scheduler Example");
    println!("===================================");

    // Configuration
    let num_workers = 8;
    let tasks_per_worker = 1000;
    let deque_capacity = 1000;
    let min_workload = 1000;
    let max_workload = 5000;

    println!("Configuration:");
    println!("  Workers: {}", num_workers);
    println!("  Tasks per worker: {}", tasks_per_worker);
    println!("  Deque capacity: {}", deque_capacity);
    println!("  Workload range: {} - {} units", min_workload, max_workload);
    println!("  Total tasks: {}\n", num_workers * tasks_per_worker);

    let start_time = Instant::now();

    // Create deques for each worker
    let deques: Vec<Arc<WorkStealingDeque<Task>>> = (0..num_workers)
        .map(|_| Arc::new(WorkStealingDeque::new(deque_capacity)))
        .collect();

    // Create barrier for synchronized start
    let barrier = Arc::new(Barrier::new(num_workers));

    // Spawn worker threads
    let mut worker_handles = vec![];
    for worker_id in 0..num_workers {
        let worker_deque = Arc::clone(&deques[worker_id]);
        let steal_targets: Vec<Arc<WorkStealingDeque<Task>>> = deques
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != worker_id)
            .map(|(_, deque)| Arc::clone(deque))
            .collect();

        let barrier = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            let mut worker = Worker::new(worker_id, worker_deque, steal_targets);
            
            // Push initial tasks
            for task_id in 0..tasks_per_worker {
                let workload = min_workload + (task_id % (max_workload - min_workload));
                let task = Task::new((worker_id * tasks_per_worker + task_id) as u64, workload);
                
                while worker.deque.push(task).is_err() {
                    thread::yield_now();
                }
            }

            worker.run(barrier)
        });

        worker_handles.push(handle);
    }

    // Collect results
    let mut all_stats = vec![];
    for handle in worker_handles {
        let stats = handle.join().unwrap();
        all_stats.push(stats);
    }

    let elapsed = start_time.elapsed();

    // Calculate aggregate statistics
    let total_tasks_executed: usize = all_stats.iter().map(|s| s.tasks_executed).sum();
    let total_tasks_stolen: usize = all_stats.iter().map(|s| s.tasks_stolen).sum();
    let total_local_tasks: usize = all_stats.iter().map(|s| s.local_tasks).sum();
    let total_steal_attempts: usize = all_stats.iter().map(|s| s.steal_attempts).sum();
    let total_work_units: usize = all_stats.iter().map(|s| s.total_work_units).sum();

    println!("\n📊 Results:");
    println!("  Total tasks executed: {}", total_tasks_executed);
    println!("  Total tasks stolen: {}", total_tasks_stolen);
    println!("  Total local tasks: {}", total_local_tasks);
    println!("  Total steal attempts: {}", total_steal_attempts);
    println!("  Total work units: {}", total_work_units);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Throughput: {:.2} tasks/sec", total_tasks_executed as f64 / elapsed.as_secs_f64());
    println!("  Work processing rate: {:.2} units/sec", total_work_units as f64 / elapsed.as_secs_f64());

    // Per-worker statistics
    println!("\n👥 Worker Statistics:");
    for (i, stats) in all_stats.iter().enumerate() {
        let steal_rate = if stats.steal_attempts > 0 {
            stats.tasks_stolen as f64 / stats.steal_attempts as f64 * 100.0
        } else {
            0.0
        };

        println!("  Worker {}: {} tasks ({} local, {} stolen), {:.1}% steal rate", 
                 i, stats.tasks_executed, stats.local_tasks, stats.tasks_stolen, steal_rate);
    }

    // Load balancing metrics
    let avg_tasks_per_worker = total_tasks_executed as f64 / num_workers as f64;
    let variance: f64 = all_stats
        .iter()
        .map(|s| {
            let diff = s.tasks_executed as f64 - avg_tasks_per_worker;
            diff * diff
        })
        .sum::<f64>() / num_workers as f64;
    let std_dev = variance.sqrt();
    let balance_efficiency = 1.0 - (std_dev / avg_tasks_per_worker);

    println!("\n⚖️ Load Balancing:");
    println!("  Average tasks per worker: {:.1}", avg_tasks_per_worker);
    println!("  Standard deviation: {:.2}", std_dev);
    println!("  Balance efficiency: {:.1}%", balance_efficiency * 100.0);

    // Verify all tasks were executed
    assert_eq!(total_tasks_executed, num_workers * tasks_per_worker, "Task count mismatch!");

    println!("\n✅ All tasks executed successfully!");
    if balance_efficiency > 0.8 {
        println!("🎯 Excellent load balancing achieved!");
    } else if balance_efficiency > 0.6 {
        println!("👍 Good load balancing achieved!");
    } else {
        println!("⚠️  Load balancing could be improved.");
    }
}
