//! Velocity runtime core: worker pool, async scheduler, and transport layer.
//!
//! This crate contains the latency-critical execution engine that coordinates
//! tool-call dispatch through pre-warmed workers, an overlap-aware scheduler,
//! and a binary wire protocol transport.

pub mod scheduler;
pub mod transport;
pub mod worker_pool;

pub use scheduler::{process_order_task, Scheduler, SchedulerError, StepResult, TaskResult, ToolCallIntent};
pub use transport::CallTrace;
pub use worker_pool::{PoolError, PoolStats, ToolExecutor, WorkerHandle, WorkerPool, WorkerPoolConfig};
