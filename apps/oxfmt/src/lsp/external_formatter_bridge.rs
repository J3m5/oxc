use std::path::Path;

use serde_json::Value;

pub type WorkspaceHandle = u32;

pub trait ExternalFormatterBridge: Send + Sync {
    /// Initialize the external formatter.
    ///
    /// # Errors
    /// Returns an error if the bridge fails to initialize.
    fn init(&self, num_threads: usize) -> Result<(), String>;
    /// Create a workspace for external formatter.
    ///
    /// # Errors
    /// Returns an error if the bridge fails to create the workspace.
    fn create_workspace(&self, root: &Path) -> Result<WorkspaceHandle, String>;
    /// Delete a workspace for external formatter.
    ///
    /// # Errors
    /// Returns an error if the bridge fails to delete the workspace.
    fn delete_workspace(&self, handle: WorkspaceHandle) -> Result<(), String>;
    /// Format a file using the external formatter.
    ///
    /// # Errors
    /// Returns an error if the bridge fails to format the provided code.
    fn format_file(
        &self,
        workspace: WorkspaceHandle,
        options: &Value,
        parser: &str,
        file: &str,
        code: &str,
    ) -> Result<String, String>;
}

#[expect(dead_code, reason = "No-op bridge kept for future/manual wiring")]
#[derive(Debug, Default)]
pub struct NoopBridge;

impl ExternalFormatterBridge for NoopBridge {
    fn init(&self, _num_threads: usize) -> Result<(), String> {
        Ok(())
    }

    fn create_workspace(&self, _root: &Path) -> Result<WorkspaceHandle, String> {
        Err("External formatter bridge not configured".to_string())
    }

    fn delete_workspace(&self, _handle: WorkspaceHandle) -> Result<(), String> {
        Ok(())
    }

    fn format_file(
        &self,
        _workspace: WorkspaceHandle,
        _options: &Value,
        _parser: &str,
        _file: &str,
        _code: &str,
    ) -> Result<String, String> {
        Err("External formatter bridge not configured".to_string())
    }
}
