pub mod core;
pub mod discovery;
pub mod symbols;
pub mod parity;

pub use core::*;
pub use discovery::*;
pub use symbols::*;
pub use parity::*;

pub fn execute_memory_world(i: core::MemoryWorldInput) -> Result<serde_json::Value, String> {
    core::execute_memory_world(i)
}
