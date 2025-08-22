use anyhow::Result;
use std::path::Path;
use tokio::fs;
use crate::types::AnalysisResult;

pub struct JsonOutput;

impl JsonOutput {
    pub fn new() -> Self {
        Self
    }
    
    pub async fn write_to_file<P: AsRef<Path>>(&self, result: &AnalysisResult, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(result)?;
        fs::write(path, json).await?;
        Ok(())
    }
}
