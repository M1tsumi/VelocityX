🚀 Major Release – v0.3.0

This release represents a significant milestone in the evolution of the library, introducing two new high-performance concurrency modules, major improvements to existing structures, and substantial upgrades to documentation, testing, and developer tooling. The focus of this release is performance, reliability, and production-grade ergonomic APIs.

✨ New Features
Concurrent HashMap Module

A fully-fledged, highly optimized concurrent hash map designed for low-latency, high-throughput workloads.

Key Capabilities

Full Concurrent Implementation:
Lock-free reads with striped lock granularity for writes, ensuring predictable performance under heavy contention.

Robin Hood Hashing:
Minimizes probe distance variance, improving lookup speed and cache efficiency.

Incremental, Non-Blocking Resizing:
Resizing occurs in small steps with guaranteed forward progress, allowing concurrent reads/writes throughout the process.

Memory-Efficient Layout:
Power-of-two capacity sizing, cache-friendly bucket design, and lazy slot allocation reduce footprint while maximizing speed.

This module is suitable for in-memory databases, real-time analytics pipelines, and high-concurrency runtimes.

Work-Stealing Deque Module

An industrial-strength work-stealing queue based on the battle-tested Chase-Lev algorithm.

Highlights

Owner/Thief Operation Model:
Fast, wait-free push/pop for the owner thread; efficient atomic steal operations for worker threads.

Circular Buffer Architecture:
Memory layout minimizes false sharing and supports near-zero-cost wraparound indexing.

Scheduler-Ready:
Built with task schedulers, parallel execution engines, and async executors in mind.

Perfect for thread pools, reactive runtimes, actor systems, and distributed task schedulers.

Enhanced MPMC Queue

The multi-producer, multi-consumer queue receives a significant upgrade.

Enhancements

Optimized Atomics:
Reduced unnecessary fences and improved memory ordering semantics.

More Expressive Errors:
Better differentiation between closed queues, capacity issues, and poisoned states.

Expanded Docs:
Clear usage patterns, edge case behavior, and performance notes.

🔧 Improvements
API Enhancements

Unified Error Model:
All concurrent structures now use consistent, predictable error enums.

Improved Documentation:
Inline explanations now include Big-O details, memory model notes, and concurrency diagrams.

More Ergonomic APIs:
Builder patterns, intuitive constructors, and improved trait coverage.

Performance Optimizations

Cache-Line Alignment:
All critical data paths aligned to prevent cross-core invalidation.

Optimized Memory Orderings:
Carefully audited Acquire/Release/SeqCst operations minimize synchronization overhead.

Reduced Allocations:
Hot paths significantly reduce heap pressure via preallocation and reuse.

Testing & Quality

Full Test Suite Expansion:
Broader coverage, including race-condition scenarios and memory ordering correctness.

Property-Based Testing:
Using proptest for extremely aggressive randomized testing.

Loom Integration:
Deterministic exploration of concurrency interleavings ensures correctness in rare edge cases.

Stress & Load Testing:
High-contention torture tests validate stability and performance guarantees.

📚 Documentation
New Examples

MPMC Queue – Basic Usage Demo

Advanced Benchmark Example

Multi-Producer/Multi-Consumer Scenarios
Complete examples with synchronization primitives and thread coordination patterns.

Enhanced README

Updated Benchmark Suite:
Comparisons with std::sync, Crossbeam, and other popular concurrency libraries.

Architecture Overview:
Full explanation of memory layouts, queue mechanics, pointer invariants, and hazard behavior.

Use Case Recommendations:
Identifies the best-fit data structure for workloads such as schedulers, message passing, parallel iterators, and more.

🛠️ Development Infrastructure
CI/CD Improvements

Multi-Platform CI: Linux, Windows, and macOS coverage.

Automated Security Scans:
Continuous vulnerability detection and dependency audit.

Codecov Integration:
Coverage percentage displayed directly in pull requests.

Performance Regression Checks:
Automated alerts for slowdowns across releases.

Build System Enhancements

Feature Flags:
Finer-grained control (std, serde, unstable, loom) to customize builds.

Optimized Release Profiles:
Tuned LTO, panic strategy, and codegen units for maximum performance.

Dependency Updates:
Migrated to the latest, stable upstream libraries and runtime dependencies.

🔄 Breaking Changes

Module Re-Organization:
map and deque modules are now public and require updated import paths.

Feature Flag Adjustments:
Some advanced functionality is gated behind explicit opt-in features.

Error Model Changes:
Consolidated error enums may require dependent code adjustments.

🐛 Bug Fixes

Resolved All Clippy Warnings:
Cleaner builds and improved code readability.

Cross-Platform Fixes:
Addressed compilation and behavioral discrepancies across OS platforms.

Memory Safety Improvements:
Fixed potential unsafe-block edge cases and pointer invariant assumptions.

Documentation Corrections:
Updated doctests, corrected imports, and clarified unsafe usage guidelines.