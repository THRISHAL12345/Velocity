//! Pre-warmed worker pool for eliminating cold-start latency.
//!
//! The pool maintains a fixed number of pre-spawned workers per tool type,
//! each holding a persistent handle to its tool executor. Workers are never
//! re-created per call — `acquire()` is a channel recv, not a spawn.
//!
//! # Performance Target
//!
//! Acquiring a warm worker under normal load must complete in under 10 microseconds
//! (no heap allocation, no syscalls beyond a channel recv).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use velocity_tools::{MockDb, MockFile, MockHttp};

/// Errors that can occur during pool operations.
#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    /// No pool exists for the requested tool name.
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    /// The pool is exhausted and the wait queue is full.
    #[error("pool exhausted for tool '{tool}': all {size} workers busy, {queued} requests queued")]
    PoolExhausted {
        tool: String,
        size: usize,
        queued: usize,
    },

    /// The pool has been shut down.
    #[error("pool is shut down")]
    Shutdown,
}

/// Statistics about the current state of a tool's worker pool.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of workers currently executing tool calls.
    pub active: usize,
    /// Number of workers idle and ready for acquisition.
    pub idle: usize,
    /// Total pool capacity.
    pub total: usize,
}

/// A handle to a worker acquired from the pool.
///
/// Holds the tool executor and metadata needed for the caller to execute
/// a tool call and then release the worker back to the pool.
pub struct WorkerHandle {
    /// Unique worker ID for tracing/instrumentation.
    pub worker_id: u64,
    /// The tool name this worker serves.
    pub tool_name: String,
    /// The underlying tool executor.
    pub executor: ToolExecutor,
    /// Channel to return the worker to the pool on release.
    return_tx: mpsc::Sender<WorkerHandle>,
}

impl std::fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerHandle")
            .field("worker_id", &self.worker_id)
            .field("tool_name", &self.tool_name)
            .finish()
    }
}

/// The actual tool executor held by a worker.
///
/// One variant per supported tool type — no dynamic dispatch overhead.
#[derive(Clone)]
pub enum ToolExecutor {
    Db(MockDb),
    Http(MockHttp),
    File(MockFile),
}

impl ToolExecutor {
    /// Executes a tool operation on the held executor.
    ///
    /// Dispatches to the appropriate mock tool without dynamic dispatch —
    /// the match is a direct jump, not a vtable lookup.
    pub async fn execute(
        &self,
        operation: &str,
        args: &[(String, String)],
    ) -> Result<String, String> {
        match self {
            ToolExecutor::Db(db) => db.execute(operation, args).await,
            ToolExecutor::Http(http) => http.execute(operation, args).await,
            ToolExecutor::File(file) => file.execute(operation, args).await,
        }
    }
}

/// Configuration for the worker pool.
#[derive(Debug, Clone)]
pub struct WorkerPoolConfig {
    /// Number of workers per tool type. Default: 16.
    pub pool_size: usize,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self { pool_size: 16 }
    }
}

/// A pre-warmed worker pool that manages workers for all tool types.
///
/// Workers are pre-spawned at initialization and recycled via channels.
/// The `acquire` fast path is a single channel recv — no allocation, no spawn.
pub struct WorkerPool {
    /// Per-tool channel receivers for available workers.
    pools: HashMap<String, Arc<Mutex<mpsc::Receiver<WorkerHandle>>>>,
    /// Per-tool return channels — kept alive to prevent channel closure.
    _return_txs: HashMap<String, mpsc::Sender<WorkerHandle>>,
    /// Per-tool pool sizes for stats computation.
    pool_sizes: HashMap<String, usize>,
    /// Global counter for active workers (acquired but not yet released).
    active_counts: HashMap<String, Arc<AtomicU64>>,
}

impl WorkerPool {
    /// Creates a new `WorkerPool` with pre-warmed workers for all three tool types.
    ///
    /// This spawns `config.pool_size` workers per tool type, each holding a
    /// persistent handle to its mock executor. The workers are immediately
    /// available for acquisition via `acquire()`.
    pub fn new(config: WorkerPoolConfig) -> Self {
        let tool_names = ["mock_db", "mock_http", "mock_file"];
        let mut pools = HashMap::new();
        let mut return_txs = HashMap::new();
        let mut pool_sizes = HashMap::new();
        let mut active_counts = HashMap::new();
        let next_worker_id = AtomicU64::new(0);

        for tool_name in &tool_names {
            let (tx, rx) = mpsc::channel(config.pool_size);
            let return_tx = tx.clone();

            // Pre-warm: create all workers and send them into the channel
            for _ in 0..config.pool_size {
                let worker_id = next_worker_id.fetch_add(1, Ordering::Relaxed);
                let executor = match *tool_name {
                    "mock_db" => ToolExecutor::Db(MockDb::with_defaults()),
                    "mock_http" => ToolExecutor::Http(MockHttp::with_defaults()),
                    "mock_file" => ToolExecutor::File(MockFile::with_defaults()),
                    _ => unreachable!(),
                };

                let handle = WorkerHandle {
                    worker_id,
                    tool_name: tool_name.to_string(),
                    executor,
                    return_tx: return_tx.clone(),
                };

                // This won't block — channel capacity equals pool size
                tx.try_send(handle).expect("channel should have capacity");
            }

            pools.insert(tool_name.to_string(), Arc::new(Mutex::new(rx)));
            return_txs.insert(tool_name.to_string(), return_tx);
            pool_sizes.insert(tool_name.to_string(), config.pool_size);
            active_counts.insert(tool_name.to_string(), Arc::new(AtomicU64::new(0)));

            info!(
                tool = *tool_name,
                pool_size = config.pool_size,
                "worker pool initialized"
            );
        }

        Self {
            pools,
            _return_txs: return_txs,
            pool_sizes,
            active_counts,
        }
    }

    /// Acquires a warm worker for the given tool name.
    ///
    /// On the fast path (workers available), this is a single channel recv
    /// with zero heap allocation. Blocks asynchronously if all workers are busy.
    pub async fn acquire(&self, tool_name: &str) -> Result<WorkerHandle, PoolError> {
        let pool = self
            .pools
            .get(tool_name)
            .ok_or_else(|| PoolError::UnknownTool(tool_name.to_string()))?;

        let active_count = self
            .active_counts
            .get(tool_name)
            .ok_or_else(|| PoolError::UnknownTool(tool_name.to_string()))?;

        let mut rx = pool.lock().await;
        match rx.recv().await {
            Some(handle) => {
                active_count.fetch_add(1, Ordering::Relaxed);
                debug!(
                    worker_id = handle.worker_id,
                    tool = tool_name,
                    "worker acquired"
                );
                Ok(handle)
            }
            None => Err(PoolError::Shutdown),
        }
    }

    /// Releases a worker back to the pool, making it available for reuse.
    ///
    /// This is a non-blocking channel send. The worker's persistent executor
    /// handle is preserved — no teardown or reinitialization occurs.
    pub fn release(&self, handle: WorkerHandle) {
        let tool_name = handle.tool_name.clone();
        let return_tx = handle.return_tx.clone();

        if let Some(active_count) = self.active_counts.get(&tool_name) {
            active_count.fetch_sub(1, Ordering::Relaxed);
        }

        debug!(
            worker_id = handle.worker_id,
            tool = %tool_name,
            "worker released"
        );

        // Return the worker to the pool via its return channel
        if return_tx.try_send(handle).is_err() {
            warn!(tool = %tool_name, "failed to return worker to pool (pool full or shut down)");
        }
    }

    /// Returns current pool statistics for the given tool.
    ///
    /// This performs no allocation — reads atomic counters and computes from
    /// known pool sizes.
    pub fn stats(&self, tool_name: &str) -> Option<PoolStats> {
        let total = *self.pool_sizes.get(tool_name)?;
        let active = self
            .active_counts
            .get(tool_name)?
            .load(Ordering::Relaxed) as usize;
        let idle = total.saturating_sub(active);

        Some(PoolStats {
            active,
            idle,
            total,
        })
    }

    /// Returns statistics for all tool pools.
    pub fn all_stats(&self) -> HashMap<String, PoolStats> {
        self.pool_sizes
            .keys()
            .filter_map(|name| Some((name.clone(), self.stats(name)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 2 });

        // Acquire a worker
        let handle = pool.acquire("mock_db").await.unwrap();
        assert_eq!(handle.tool_name, "mock_db");

        // Stats should show 1 active, 1 idle
        let stats = pool.stats("mock_db").unwrap();
        assert_eq!(stats.active, 1);
        assert_eq!(stats.idle, 1);
        assert_eq!(stats.total, 2);

        // Release worker
        pool.release(handle);

        // Give the channel a moment to process
        tokio::task::yield_now().await;

        // Stats should show 0 active, 2 idle
        let stats = pool.stats("mock_db").unwrap();
        assert_eq!(stats.active, 0);
        assert_eq!(stats.idle, 2);
    }

    #[tokio::test]
    async fn test_acquire_all_workers() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 3 });

        let h1 = pool.acquire("mock_http").await.unwrap();
        let h2 = pool.acquire("mock_http").await.unwrap();
        let h3 = pool.acquire("mock_http").await.unwrap();

        let stats = pool.stats("mock_http").unwrap();
        assert_eq!(stats.active, 3);
        assert_eq!(stats.idle, 0);

        pool.release(h1);
        pool.release(h2);
        pool.release(h3);
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let pool = WorkerPool::new(WorkerPoolConfig::default());
        let result = pool.acquire("nonexistent_tool").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PoolError::UnknownTool(_)));
    }

    #[tokio::test]
    async fn test_worker_execute() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 1 });
        let handle = pool.acquire("mock_db").await.unwrap();

        let args = vec![("account_id".to_string(), "ACC-999".to_string())];
        let result = handle.executor.execute("lookup_account", &args).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("ACC-999"));

        pool.release(handle);
    }

    #[tokio::test]
    async fn test_all_tool_types() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 1 });

        // DB
        let h = pool.acquire("mock_db").await.unwrap();
        pool.release(h);

        // HTTP
        let h = pool.acquire("mock_http").await.unwrap();
        pool.release(h);

        // File
        let h = pool.acquire("mock_file").await.unwrap();
        pool.release(h);
    }

    #[tokio::test]
    async fn test_all_stats() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 4 });
        let stats = pool.all_stats();
        assert_eq!(stats.len(), 3);
        for s in stats.values() {
            assert_eq!(s.total, 4);
            assert_eq!(s.idle, 4);
            assert_eq!(s.active, 0);
        }
    }

    #[tokio::test]
    async fn test_worker_reuse() {
        let pool = WorkerPool::new(WorkerPoolConfig { pool_size: 1 });

        // Acquire, execute, release, repeat — same worker should be reused
        for _ in 0..5 {
            let handle = pool.acquire("mock_file").await.unwrap();
            let result = handle.executor.execute("read", &[]).await;
            assert!(result.is_ok());
            pool.release(handle);
            tokio::task::yield_now().await;
        }
    }
}
