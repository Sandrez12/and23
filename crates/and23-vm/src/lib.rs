//! Máquina virtual de pila de And23 y la sesión que une compilador y VM.

mod session;
mod value;
mod vm;

pub use session::Session;
pub use value::Value;
pub use vm::{Vm, DEFAULT_MAX_DEPTH};
