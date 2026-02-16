pub mod types;
pub mod handlers;
pub mod logic;

// Re-exportamos los tipos y la lógica principal para mantener la compatibilidad
pub use types::{CoordinatorCommand, WorkerMessage};
pub use logic::Coordinator;
