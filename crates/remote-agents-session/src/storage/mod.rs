//! Storage implementations.

#[cfg(feature = "memory")]
pub mod memory;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "memory")]
pub use memory::MemoryStorage;
