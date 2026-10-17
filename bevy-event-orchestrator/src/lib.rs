#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

mod bus;
mod event;
mod plugin;
mod slot;

pub use bus::*;
pub use event::*;
pub use plugin::*;
pub use slot::*;

#[cfg(test)]
mod tests;
