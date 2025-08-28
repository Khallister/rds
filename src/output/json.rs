use crate::types::AnalysisResult;
use anyhow::Result;
use std::path::Path;
use tokio::fs;

pub struct JsonOutput;

impl JsonOutput {
    pub fn new() -> Self {
        Self
    }

    /// Writes the given `AnalysisResult` to the specified file path in pretty-printed JSON format asynchronously.
    ///
    /// # Arguments
    ///
    /// * `result` - A reference to the `AnalysisResult` to serialize.
    /// * `path` - The file path where the JSON output will be written.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails.
    pub async fn write_to_file<P: AsRef<Path>>(
        &self,
        result: &AnalysisResult,
        path: P,
    ) -> Result<()> {
        let json = serde_json::to_string_pretty(result)?;
        fs::write(path, json.as_bytes()).await?;
        Ok(())
    }
}
