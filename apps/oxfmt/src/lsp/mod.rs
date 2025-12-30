use std::sync::Arc;

use oxc_language_server::run_server;
use serde_json::Value;
use tokio::task::block_in_place;

use crate::core::{
    ExternalFormatter, JsCreateWorkspaceCb, JsDeleteWorkspaceCb, JsFormatEmbeddedCb, JsFormatFileCb,
    JsInitExternalFormatterCb,
};

mod external_formatter_bridge;
mod options;
mod server_formatter;
#[cfg(test)]
mod tester;

const FORMAT_CONFIG_FILES: &[&str; 2] = &[".oxfmtrc.json", ".oxfmtrc.jsonc"];

use external_formatter_bridge::ExternalFormatterBridge;

struct NapiExternalFormatterBridge {
    formatter: ExternalFormatter,
}

impl ExternalFormatterBridge for NapiExternalFormatterBridge {
    fn init(&self, num_threads: usize) -> Result<(), String> {
        block_in_place(|| self.formatter.init(num_threads).map(|_| ()))
    }

    fn create_workspace(
        &self,
        root: &std::path::Path,
    ) -> Result<external_formatter_bridge::WorkspaceHandle, String> {
        block_in_place(|| {
            self.formatter
                .create_workspace(root.to_string_lossy().as_ref())
        })
    }

    fn delete_workspace(
        &self,
        handle: external_formatter_bridge::WorkspaceHandle,
    ) -> Result<(), String> {
        block_in_place(|| self.formatter.delete_workspace(handle))
    }

    fn format_file(
        &self,
        workspace: external_formatter_bridge::WorkspaceHandle,
        options: &Value,
        parser: &str,
        file: &str,
        code: &str,
    ) -> Result<String, String> {
        block_in_place(|| self.formatter.format_file(workspace, options, parser, file, code))
    }
}

/// Run the language server
pub async fn run_lsp(
    init_external_formatter_cb: JsInitExternalFormatterCb,
    format_embedded_cb: JsFormatEmbeddedCb,
    format_file_cb: JsFormatFileCb,
    create_workspace_cb: JsCreateWorkspaceCb,
    delete_workspace_cb: JsDeleteWorkspaceCb,
) {
    let external_formatter =
        ExternalFormatter::new(
            init_external_formatter_cb,
            format_embedded_cb,
            format_file_cb,
            create_workspace_cb,
            delete_workspace_cb,
        );
    let bridge = Arc::new(NapiExternalFormatterBridge { formatter: external_formatter });

    run_server(
        "oxfmt".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
        vec![Box::new(server_formatter::ServerFormatterBuilder::new(Some(bridge)))],
    )
    .await;
}
