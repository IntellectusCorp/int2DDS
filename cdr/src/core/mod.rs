// Core serialization modules
mod alignment;
mod bounded_types;
mod collections;
mod endianness;
mod errors;
mod primitive;
mod reader;
pub mod xcdr;

// Re-export everything from submodules
pub use alignment::*;
pub use bounded_types::*;
pub use collections::*;
pub use endianness::*;
pub use errors::*;
pub use primitive::*;
pub use reader::*;

// Common buffer management trait for serializers
pub trait BufferManager {
    fn into_bytes(self) -> Vec<u8>;
    fn as_bytes(&self) -> &[u8];
    fn reset(&mut self);
}
