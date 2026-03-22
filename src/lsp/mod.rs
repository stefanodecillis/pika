pub mod capabilities;
pub mod client;
pub mod registry;
pub mod types;

pub use client::LspClient;
pub use registry::{LspRegistry, ServerConfig};
