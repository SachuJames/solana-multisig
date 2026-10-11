use solana_program::entrypoint;

pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;

pub use processor::process_instruction;

entrypoint!(process_instruction);
