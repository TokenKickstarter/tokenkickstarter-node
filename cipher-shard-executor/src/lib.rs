//! Cipher shard executor (M2): batch, Merkle root, DA write, anchor payload.

pub mod anchor;
pub mod batch;
pub mod da;
pub mod embed;
pub mod engine;
pub mod merkle;
pub mod server;

pub use batch::{CipherBatchMessage, DataAvailabilityBlob};
pub use embed::{run_embedded, EmbeddedExecutorConfig};
pub use engine::{new_shared_engine, BatchEngine, SharedEngine};
pub use merkle::compute_dag_root;
