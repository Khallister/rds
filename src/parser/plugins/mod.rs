pub mod javascript;
pub mod vue;

pub use javascript::JavaScriptParser;
pub use vue::VueParser;

#[cfg(test)]
mod tests;
