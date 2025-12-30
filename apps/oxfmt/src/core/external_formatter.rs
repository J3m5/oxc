use std::sync::Arc;

use napi::{
    Status,
    bindgen_prelude::{FnArgs, Promise, block_on},
    threadsafe_function::ThreadsafeFunction,
};
use serde_json::Value;

/// Type alias for the init external formatter callback function signature.
/// Takes num_threads as argument and returns plugin languages.
pub type JsInitExternalFormatterCb = ThreadsafeFunction<
    // Input arguments
    FnArgs<(u32,)>, // (num_threads,)
    // Return type (what JS function returns)
    Promise<Vec<String>>,
    // Arguments (repeated)
    FnArgs<(u32,)>,
    // Error status
    Status,
    // CalleeHandled
    false,
>;

/// Type alias for the callback function signature.
/// Takes (options, tag_name, code) as separate arguments and returns formatted code.
pub type JsFormatEmbeddedCb = ThreadsafeFunction<
    // Input arguments
    FnArgs<(Value, String, String)>, // (options, tag_name, code)
    // Return type (what JS function returns)
    Promise<String>,
    // Arguments (repeated)
    FnArgs<(Value, String, String)>,
    // Error status
    Status,
    // CalleeHandled
    false,
>;

/// Type alias for the callback function signature.
/// Takes (workspace_id, options, parser_name, file_name, code) as separate arguments
/// and returns formatted code.
pub type JsFormatFileCb = ThreadsafeFunction<
    // Input arguments
    FnArgs<(u32, Value, String, String, String)>, // (workspace_id, options, parser_name, file_name, code)
    // Return type (what JS function returns)
    Promise<String>,
    // Arguments (repeated)
    FnArgs<(u32, Value, String, String, String)>,
    // Error status
    Status,
    // CalleeHandled
    false,
>;

/// Type alias for the create workspace callback function signature.
/// Takes (directory) and returns a workspace id.
pub type JsCreateWorkspaceCb = ThreadsafeFunction<
    // Input arguments
    FnArgs<(String,)>, // (directory)
    // Return type (what JS function returns)
    Promise<u32>,
    // Arguments (repeated)
    FnArgs<(String,)>,
    // Error status
    Status,
    // CalleeHandled
    false,
>;

/// Type alias for the delete workspace callback function signature.
/// Takes (workspace_id) and returns void.
pub type JsDeleteWorkspaceCb = ThreadsafeFunction<
    // Input arguments
    FnArgs<(u32,)>, // (workspace_id)
    // Return type (what JS function returns)
    Promise<()>,
    // Arguments (repeated)
    FnArgs<(u32,)>,
    // Error status
    Status,
    // CalleeHandled
    false,
>;

/// Callback function type for formatting embedded code with config.
/// Takes (options, tag_name, code) and returns formatted code or an error.
type FormatEmbeddedWithConfigCallback =
    Arc<dyn Fn(&Value, &str, &str) -> Result<String, String> + Send + Sync>;

/// Callback function type for formatting files with config.
/// Takes (workspace_id, options, parser_name, file_name, code) and returns formatted code or an error.
type FormatFileWithConfigCallback =
    Arc<dyn Fn(u32, &Value, &str, &str, &str) -> Result<String, String> + Send + Sync>;

/// Callback function type for init external formatter.
/// Takes num_threads and returns plugin languages.
type InitExternalFormatterCallback =
    Arc<dyn Fn(usize) -> Result<Vec<String>, String> + Send + Sync>;

/// Callback function type for creating a workspace.
/// Takes (directory) and returns a workspace id.
type CreateWorkspaceCallback = Arc<dyn Fn(&str) -> Result<u32, String> + Send + Sync>;

/// Callback function type for deleting a workspace.
/// Takes (workspace_id) and returns void.
type DeleteWorkspaceCallback = Arc<dyn Fn(u32) -> Result<(), String> + Send + Sync>;

/// External formatter that wraps a JS callback.
#[derive(Clone)]
pub struct ExternalFormatter {
    pub init: InitExternalFormatterCallback,
    pub format_embedded: FormatEmbeddedWithConfigCallback,
    pub format_file: FormatFileWithConfigCallback,
    pub create_workspace: CreateWorkspaceCallback,
    pub delete_workspace: DeleteWorkspaceCallback,
}

impl std::fmt::Debug for ExternalFormatter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalFormatter")
            .field("init", &"<callback>")
            .field("format_embedded", &"<callback>")
            .field("format_file", &"<callback>")
            .field("create_workspace", &"<callback>")
            .field("delete_workspace", &"<callback>")
            .finish()
    }
}

impl ExternalFormatter {
    /// Create an [`ExternalFormatter`] from JS callbacks.
    pub fn new(
        init_cb: JsInitExternalFormatterCb,
        format_embedded_cb: JsFormatEmbeddedCb,
        format_file_cb: JsFormatFileCb,
        create_workspace_cb: JsCreateWorkspaceCb,
        delete_workspace_cb: JsDeleteWorkspaceCb,
    ) -> Self {
        let rust_init = wrap_init_external_formatter(init_cb);
        let rust_format_embedded = wrap_format_embedded(format_embedded_cb);
        let rust_format_file = wrap_format_file(format_file_cb);
        let rust_create_workspace = wrap_create_workspace(create_workspace_cb);
        let rust_delete_workspace = wrap_delete_workspace(delete_workspace_cb);
        Self {
            init: rust_init,
            format_embedded: rust_format_embedded,
            format_file: rust_format_file,
            create_workspace: rust_create_workspace,
            delete_workspace: rust_delete_workspace,
        }
    }

    /// Initialize external formatter using the JS callback.
    pub fn init(&self, num_threads: usize) -> Result<Vec<String>, String> {
        (self.init)(num_threads)
    }

    /// Convert this external formatter to the oxc_formatter::EmbeddedFormatter type.
    /// The options is captured in the closure and passed to JS on each call.
    pub fn to_embedded_formatter(&self, options: Value) -> oxc_formatter::EmbeddedFormatter {
        let format_embedded = Arc::clone(&self.format_embedded);
        let callback =
            Arc::new(move |tag_name: &str, code: &str| (format_embedded)(&options, tag_name, code));
        oxc_formatter::EmbeddedFormatter::new(callback)
    }

    /// Format non-js file using the JS callback.
    pub fn format_file(
        &self,
        workspace_id: u32,
        options: &Value,
        parser_name: &str,
        file_name: &str,
        code: &str,
    ) -> Result<String, String> {
        (self.format_file)(workspace_id, options, parser_name, file_name, code)
    }

    /// Create a workspace for external formatter.
    pub fn create_workspace(&self, directory: &str) -> Result<u32, String> {
        (self.create_workspace)(directory)
    }

    /// Delete a workspace for external formatter.
    pub fn delete_workspace(&self, workspace_id: u32) -> Result<(), String> {
        (self.delete_workspace)(workspace_id)
    }
}

// ---

// NOTE: These methods are all wrapped by `block_on` to run the async JS calls in a blocking manner.
//
// When called from `rayon` worker threads (Mode::Cli), this works fine.
// Because `rayon` threads are separate from the `tokio` runtime.
//
// However, in cases like `--stdin-filepath` or Node.js API calls,
// where already inside an async context (the `napi`'s `async` function),
// calling `block_on` directly would cause issues with nested async runtime access.
//
// Therefore, `block_in_place()` is used at the call site
// to temporarily convert the current async task into a blocking context.

/// Wrap JS `initExternalFormatter` callback as a normal Rust function.
fn wrap_init_external_formatter(cb: JsInitExternalFormatterCb) -> InitExternalFormatterCallback {
    Arc::new(move |num_threads: usize| {
        block_on(async {
            #[expect(clippy::cast_possible_truncation)]
            let status = cb.call_async(FnArgs::from((num_threads as u32,))).await;
            match status {
                Ok(promise) => match promise.await {
                    Ok(languages) => Ok(languages),
                    Err(err) => Err(format!("JS initExternalFormatter promise rejected: {err}")),
                },
                Err(err) => Err(format!("Failed to call JS initExternalFormatter callback: {err}")),
            }
        })
    })
}

/// Wrap JS `formatEmbeddedCode` callback as a normal Rust function.
fn wrap_format_embedded(cb: JsFormatEmbeddedCb) -> FormatEmbeddedWithConfigCallback {
    Arc::new(move |options: &Value, tag_name: &str, code: &str| {
        block_on(async {
            let status = cb
                .call_async(FnArgs::from((options.clone(), tag_name.to_string(), code.to_string())))
                .await;
            match status {
                Ok(promise) => match promise.await {
                    Ok(formatted_code) => Ok(formatted_code),
                    Err(err) => {
                        Err(format!("JS formatter promise rejected for tag '{tag_name}': {err}"))
                    }
                },
                Err(err) => Err(format!(
                    "Failed to call JS formatting callback for tag '{tag_name}': {err}"
                )),
            }
        })
    })
}

/// Wrap JS `formatFile` callback as a normal Rust function.
fn wrap_format_file(cb: JsFormatFileCb) -> FormatFileWithConfigCallback {
    Arc::new(
        move |workspace_id: u32, options: &Value, parser_name: &str, file_name: &str, code: &str| {
            block_on(async {
                let status = cb
                    .call_async(FnArgs::from((
                        workspace_id,
                        options.clone(),
                        parser_name.to_string(),
                        file_name.to_string(),
                        code.to_string(),
                    )))
                    .await;
                match status {
                    Ok(promise) => match promise.await {
                        Ok(formatted_code) => Ok(formatted_code),
                        Err(err) => Err(format!(
                            "JS formatFile promise rejected for file: '{file_name}', parser: '{parser_name}': {err}"
                        )),
                    },
                    Err(err) => Err(format!(
                        "Failed to call JS formatFile callback for file: '{file_name}', parser: '{parser_name}': {err}"
                    )),
                }
            })
        },
    )
}

/// Wrap JS `createWorkspace` callback as a normal Rust function.
fn wrap_create_workspace(cb: JsCreateWorkspaceCb) -> CreateWorkspaceCallback {
    Arc::new(move |directory: &str| {
        block_on(async {
            let status = cb.call_async(FnArgs::from((directory.to_string(),))).await;
            match status {
                Ok(promise) => match promise.await {
                    Ok(workspace_id) => Ok(workspace_id),
                    Err(err) => Err(format!(
                        "JS createWorkspace promise rejected for directory: '{directory}': {err}"
                    )),
                },
                Err(err) => Err(format!(
                    "Failed to call JS createWorkspace callback for directory: '{directory}': {err}"
                )),
            }
        })
    })
}

/// Wrap JS `deleteWorkspace` callback as a normal Rust function.
fn wrap_delete_workspace(cb: JsDeleteWorkspaceCb) -> DeleteWorkspaceCallback {
    Arc::new(move |workspace_id: u32| {
        block_on(async {
            let status = cb.call_async(FnArgs::from((workspace_id,))).await;
            match status {
                Ok(promise) => match promise.await {
                    Ok(()) => Ok(()),
                    Err(err) => Err(format!(
                        "JS deleteWorkspace promise rejected for workspace {workspace_id}: {err}"
                    )),
                },
                Err(err) => Err(format!(
                    "Failed to call JS deleteWorkspace callback for workspace {workspace_id}: {err}"
                )),
            }
        })
    })
}
