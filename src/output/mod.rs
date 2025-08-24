pub mod console;
pub mod json;

pub use console::ConsoleOutput;
pub use json::JsonOutput;

#[cfg(test)]
mod tests;
