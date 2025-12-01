//! Performance Metrics Module
//!
//! This module provides standardized performance monitoring and metrics collection
//! for all VelocityX data structures. It offers insights into contention, memory usage,
//! and operational health without impacting performance.

#[cfg(feature = "std")]
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(feature = "std")]
use std::time::Duration;

/// Core performance metrics for all data structures
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    /// Total number of operations performed
    pub total_operations: u64,
    /// Number of successful operations
    pub successful_operations: u64,
    /// Number of failed operations
    pub failed_operations: u64,
    /// Number of contended operations (had to wait/retry)
    pub contended_operations: u64,
    /// Average operation time in nanoseconds
    pub avg_operation_time_ns: u64,
    /// Maximum operation time in nanoseconds
    pub max_operation_time_ns: u64,
    /// Current memory usage in bytes
    pub memory_usage_bytes: usize,
    /// Peak memory usage in bytes
    pub peak_memory_usage_bytes: usize,
}

impl PerformanceMetrics {
    /// Calculate success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            (self.successful_operations as f64 / self.total_operations as f64) * 100.0
        }
    }

    /// Calculate contention rate as percentage
    pub fn contention_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            (self.contended_operations as f64 / self.total_operations as f64) * 100.0
        }
    }

    /// Calculate failure rate as percentage
    pub fn failure_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            (self.failed_operations as f64 / self.total_operations as f64) * 100.0
        }
    }

    /// Get average operation time as Duration
    pub fn avg_operation_time(&self) -> Duration {
        Duration::from_nanos(self.avg_operation_time_ns)
    }

    /// Get maximum operation time as Duration
    pub fn max_operation_time(&self) -> Duration {
        Duration::from_nanos(self.max_operation_time_ns)
    }
}

/// Internal atomic metrics collection
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct AtomicMetrics {
    pub total_operations: AtomicU64,
    pub successful_operations: AtomicU64,
    pub failed_operations: AtomicU64,
    pub contended_operations: AtomicU64,
    pub total_time_ns: AtomicU64,
    pub max_time_ns: AtomicU64,
    pub memory_usage: AtomicUsize,
    pub peak_memory_usage: AtomicUsize,
}

#[cfg(feature = "std")]
impl Default for AtomicMetrics {
    fn default() -> Self {
        Self {
            total_operations: AtomicU64::new(0),
            successful_operations: AtomicU64::new(0),
            failed_operations: AtomicU64::new(0),
            contended_operations: AtomicU64::new(0),
            total_time_ns: AtomicU64::new(0),
            max_time_ns: AtomicU64::new(0),
            memory_usage: AtomicUsize::new(0),
            peak_memory_usage: AtomicUsize::new(0),
        }
    }
}

#[cfg(feature = "std")]
impl AtomicMetrics {
    /// Record a successful operation with its duration
    pub fn record_success(&self, duration: Duration) {
        let duration_ns = duration.as_nanos() as u64;
        
        self.total_operations.fetch_add(1, Ordering::Relaxed);
        self.successful_operations.fetch_add(1, Ordering::Relaxed);
        self.total_time_ns.fetch_add(duration_ns, Ordering::Relaxed);
        
        // Update max time if this operation was slower
        let mut current_max = self.max_time_ns.load(Ordering::Relaxed);
        while duration_ns > current_max {
            match self.max_time_ns.compare_exchange_weak(
                current_max,
                duration_ns,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }
    }

    /// Record a failed operation
    pub fn record_failure(&self) {
        self.total_operations.fetch_add(1, Ordering::Relaxed);
        self.failed_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a contended operation
    pub fn record_contention(&self) {
        self.contended_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Update memory usage
    pub fn update_memory_usage(&self, usage: usize) {
        let _current_usage = self.memory_usage.swap(usage, Ordering::Relaxed);
        
        // Update peak usage if current usage is higher
        let mut current_peak = self.peak_memory_usage.load(Ordering::Relaxed);
        while usage > current_peak {
            match self.peak_memory_usage.compare_exchange_weak(
                current_peak,
                usage,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_peak = x,
            }
        }
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> PerformanceMetrics {
        let total_ops = self.total_operations.load(Ordering::Relaxed);
        let successful_ops = self.successful_operations.load(Ordering::Relaxed);
        let failed_ops = self.failed_operations.load(Ordering::Relaxed);
        let contended_ops = self.contended_operations.load(Ordering::Relaxed);
        let total_time = self.total_time_ns.load(Ordering::Relaxed);
        let max_time = self.max_time_ns.load(Ordering::Relaxed);
        let memory_usage = self.memory_usage.load(Ordering::Relaxed);
        let peak_memory_usage = self.peak_memory_usage.load(Ordering::Relaxed);

        PerformanceMetrics {
            total_operations: total_ops,
            successful_operations: successful_ops,
            failed_operations: failed_ops,
            contended_operations: contended_ops,
            avg_operation_time_ns: if total_ops > 0 { total_time / total_ops } else { 0 },
            max_operation_time_ns: max_time,
            memory_usage_bytes: memory_usage,
            peak_memory_usage_bytes: peak_memory_usage,
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.total_operations.store(0, Ordering::Relaxed);
        self.successful_operations.store(0, Ordering::Relaxed);
        self.failed_operations.store(0, Ordering::Relaxed);
        self.contended_operations.store(0, Ordering::Relaxed);
        self.total_time_ns.store(0, Ordering::Relaxed);
        self.max_time_ns.store(0, Ordering::Relaxed);
        // Keep memory usage as is since it reflects current state
    }
}

/// Trait for data structures that support performance metrics
pub trait MetricsCollector {
    /// Get current performance metrics
    fn metrics(&self) -> PerformanceMetrics;
    
    /// Reset all metrics
    fn reset_metrics(&self);
    
    /// Enable or disable metrics collection
    fn set_metrics_enabled(&self, enabled: bool);
    
    /// Check if metrics collection is enabled
    fn is_metrics_enabled(&self) -> bool;
}

#[cfg(not(feature = "std"))]
pub struct AtomicMetrics;

#[cfg(not(feature = "std"))]
impl Default for AtomicMetrics {
    fn default() -> Self {
        Self
    }
}

#[cfg(not(feature = "std"))]
impl AtomicMetrics {
    pub fn record_success(&self, _duration: core::time::Duration) {}
    pub fn record_failure(&self) {}
    pub fn record_contention(&self) {}
    pub fn update_memory_usage(&self, _usage: usize) {}
    pub fn snapshot(&self) -> PerformanceMetrics {
        PerformanceMetrics::default()
    }
    pub fn reset(&self) {}
}

#[cfg(not(feature = "std"))]
pub trait MetricsCollector {
    fn metrics(&self) -> PerformanceMetrics { PerformanceMetrics::default() }
    fn reset_metrics(&self) {}
    fn set_metrics_enabled(&self, _enabled: bool) {}
    fn is_metrics_enabled(&self) -> bool { false }
}
