use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use log::{debug, warn};
use oxc_allocator::Allocator;
use oxc_data_structures::rope::{Rope, get_line_column};
use oxc_formatter::{Formatter, enable_jsx_source_type, get_parse_options};
use oxc_parser::Parser;
use tower_lsp_server::ls_types::{Pattern, Position, Range, ServerCapabilities, TextEdit, Uri};

use crate::lsp::{
    FORMAT_CONFIG_FILES,
    external_formatter_bridge::ExternalFormatterBridge,
    options::FormatOptions as LSPFormatOptions,
};
use crate::lsp::external_formatter_bridge::WorkspaceHandle;
use crate::core::{
    ConfigResolver, FormatFileStrategy, ResolvedOptions, resolve_editorconfig_path,
};
use oxc_language_server::{Capabilities, Tool, ToolBuilder, ToolRestartChanges};

use sort_package_json::SortOptions;

#[derive(Clone, Default)]
pub struct ServerFormatterBuilder {
    external_bridge: Option<Arc<dyn ExternalFormatterBridge>>,
}

impl ServerFormatterBuilder {
    /// # Panics
    /// Panics if the root URI cannot be converted to a file path.
    pub fn new(external_bridge: Option<Arc<dyn ExternalFormatterBridge>>) -> Self {
        Self { external_bridge }
    }

    /// # Panics
    /// Panics if the root URI cannot be converted to a file path.
    pub fn build(
        root_uri: &Uri,
        options: serde_json::Value,
        external_bridge: Option<Arc<dyn ExternalFormatterBridge>>,
    ) -> ServerFormatter {
        let options = match serde_json::from_value::<LSPFormatOptions>(options) {
            Ok(opts) => opts,
            Err(err) => {
                warn!(
                    "Failed to deserialize LSPFormatOptions from JSON: {err}, falling back to default options"
                );
                LSPFormatOptions::default()
            }
        };

        let root_path = root_uri.to_file_path().unwrap();
        let (config_resolver, ignore_patterns) =
            Self::resolve_config(&root_path, options.config_path.as_ref());

        let gitignore_glob = match Self::create_ignore_globs(&root_path, &ignore_patterns) {
                Ok(glob) => Some(glob),
                Err(err) => {
                    warn!(
                        "Failed to create gitignore globs: {err}, proceeding without ignore globs"
                    );
                    None
                }
            };

        let (external_bridge, workspace_handle) =
            Self::init_external_formatter(&root_path, external_bridge);
        ServerFormatter::new(config_resolver, gitignore_glob, external_bridge, workspace_handle)
    }
}

impl ToolBuilder for ServerFormatterBuilder {
    fn server_capabilities(
        &self,
        capabilities: &mut ServerCapabilities,
        _backend_capabilities: &Capabilities,
    ) {
        capabilities.document_formatting_provider =
            Some(tower_lsp_server::ls_types::OneOf::Left(true));
    }
    fn build_boxed(&self, root_uri: &Uri, options: serde_json::Value) -> Box<dyn Tool> {
        Box::new(ServerFormatterBuilder::build(root_uri, options, self.external_bridge.clone()))
    }
}

impl ServerFormatterBuilder {
    fn resolve_config(
        root_path: &Path,
        config_path: Option<&String>,
    ) -> (ConfigResolver, Vec<String>) {
        let oxfmtrc_path = Self::find_config_path(root_path, config_path);

        let editorconfig_path = resolve_editorconfig_path(root_path);
        let mut config_resolver =
            match ConfigResolver::from_config_paths(
                root_path,
                oxfmtrc_path.as_deref(),
                editorconfig_path.as_deref(),
            ) {
                Ok(resolver) => resolver,
                Err(err) => {
                    warn!("Failed to load configuration file: {err}, using default config");
                    ConfigResolver::from_config_paths(root_path, None, None)
                        .expect("default config should always load")
                }
            };

        let ignore_patterns = match config_resolver.build_and_validate() {
            Ok(patterns) => patterns,
            Err(err) => {
                warn!("Failed to parse configuration: {err}, using default config");
                let mut fallback = ConfigResolver::from_config_paths(root_path, None, None)
                    .expect("default config should always load");
                let patterns = fallback.build_and_validate().unwrap_or_default();
                config_resolver = fallback;
                patterns
            }
        };

        (config_resolver, ignore_patterns)
    }

    fn find_config_path(root_path: &Path, config_path: Option<&String>) -> Option<PathBuf> {
        if let Some(config_path) = config_path.filter(|s| !s.is_empty()) {
            let config = root_path.join(config_path);
            if config.try_exists().is_ok_and(|exists| exists) {
                return Some(config);
            }

            warn!(
                "Config file not found: {}, searching for `{}` in the root path",
                config.display(),
                FORMAT_CONFIG_FILES.join(", ")
            );
        }

        FORMAT_CONFIG_FILES.iter().find_map(|&file| {
            let config = root_path.join(file);
            config.try_exists().is_ok_and(|exists| exists).then_some(config)
        })
    }

    fn create_ignore_globs(
        root_path: &Path,
        ignore_patterns: &[String],
    ) -> Result<Gitignore, String> {
        let mut builder = GitignoreBuilder::new(root_path);
        for ignore_path in &load_ignore_paths(root_path) {
            if builder.add(ignore_path).is_some() {
                return Err(format!("Failed to add ignore file: {}", ignore_path.display()));
            }
        }
        for pattern in ignore_patterns {
            builder
                .add_line(None, pattern)
                .map_err(|e| format!("Invalid ignore pattern: {pattern}: {e}"))?;
        }

        builder.build().map_err(|_| "Failed to build ignore globs".to_string())
    }

    fn init_external_formatter(
        root_path: &Path,
        external_bridge: Option<Arc<dyn ExternalFormatterBridge>>,
    ) -> (Option<Arc<dyn ExternalFormatterBridge>>, Option<WorkspaceHandle>) {
        let mut external_bridge = external_bridge;
        let mut workspace_handle = None;

        if let Some(bridge) = external_bridge.as_ref()
            && let Err(err) = bridge.init(1)
        {
            debug!("Failed to initialize external formatter bridge: {err}");
            external_bridge = None;
        }

        if let Some(bridge) = external_bridge.as_ref() {
            match bridge.create_workspace(root_path) {
                Ok(handle) => {
                    workspace_handle = Some(handle);
                }
                Err(err) => {
                    debug!("Failed to create external formatter workspace: {err}");
                    external_bridge = None;
                }
            }
        }

        (external_bridge, workspace_handle)
    }
}
pub struct ServerFormatter {
    config_resolver: ConfigResolver,
    gitignore_glob: Option<Gitignore>,
    workspace_handle: Option<WorkspaceHandle>,
    external_bridge: Option<Arc<dyn ExternalFormatterBridge>>,
}

impl Tool for ServerFormatter {
    fn name(&self) -> &'static str {
        "formatter"
    }
    /// # Panics
    /// Panics if the root URI cannot be converted to a file path.
    fn handle_configuration_change(
        &self,
        builder: &dyn ToolBuilder,
        root_uri: &Uri,
        old_options_json: &serde_json::Value,
        new_options_json: serde_json::Value,
    ) -> ToolRestartChanges {
        let old_option = match serde_json::from_value::<LSPFormatOptions>(old_options_json.clone())
        {
            Ok(opts) => opts,
            Err(e) => {
                warn!(
                    "Failed to deserialize LSPFormatOptions from JSON: {e}. Falling back to default options."
                );
                LSPFormatOptions::default()
            }
        };

        let new_option = match serde_json::from_value::<LSPFormatOptions>(new_options_json.clone())
        {
            Ok(opts) => opts,
            Err(e) => {
                warn!(
                    "Failed to deserialize LSPFormatOptions from JSON: {e}. Falling back to default options."
                );
                LSPFormatOptions::default()
            }
        };

        if old_option == new_option {
            return ToolRestartChanges { tool: None, watch_patterns: None };
        }

        let new_formatter = builder.build_boxed(root_uri, new_options_json.clone());
        let watch_patterns = new_formatter.get_watcher_patterns(new_options_json);
        ToolRestartChanges { tool: Some(new_formatter), watch_patterns: Some(watch_patterns) }
    }

    fn get_watcher_patterns(&self, options: serde_json::Value) -> Vec<Pattern> {
        let options = match serde_json::from_value::<LSPFormatOptions>(options) {
            Ok(opts) => opts,
            Err(e) => {
                warn!(
                    "Failed to deserialize LSPFormatOptions from JSON: {e}. Falling back to default options."
                );
                LSPFormatOptions::default()
            }
        };

        if let Some(config_path) = options.config_path.as_ref().filter(|s| !s.is_empty()) {
            return vec![config_path.clone()];
        }

        FORMAT_CONFIG_FILES.iter().map(|file| (*file).to_string()).collect()
    }

    fn handle_watched_file_change(
        &self,
        builder: &dyn ToolBuilder,
        _changed_uri: &Uri,
        root_uri: &Uri,
        options: serde_json::Value,
    ) -> ToolRestartChanges {
        // TODO: Check if the changed file is actually a config file

        let new_formatter = builder.build_boxed(root_uri, options);

        ToolRestartChanges {
            tool: Some(new_formatter),
            // TODO: update watch patterns if config_path changed
            watch_patterns: None,
        }
    }

    fn run_format(&self, uri: &Uri, content: Option<&str>) -> Option<Vec<TextEdit>> {
        // Formatter is disabled

        let path: PathBuf = uri.to_file_path()?.into();

        if self.is_ignored(&path) {
            debug!("File is ignored: {}", path.display());
            return None;
        }

        // Declaring Variable to satisfy borrow checker
        let file_content;
        let source_text = if let Some(content) = content {
            content
        } else {
            #[cfg(not(all(test, windows)))]
            {
                file_content = std::fs::read_to_string(&path).ok()?;
            }
            #[cfg(all(test, windows))]
            #[expect(clippy::disallowed_methods)] // no `cow_replace` in tests are fine
            // On Windows, convert CRLF to LF for consistent formatting results
            {
                file_content = std::fs::read_to_string(&path).ok()?.replace("\r\n", "\n");
            }
            &file_content
        };

        let strategy = FormatFileStrategy::try_from(path.clone()).ok()?;
        match strategy {
            FormatFileStrategy::OxcFormatter { source_type, .. } => {
                let ResolvedOptions::OxcFormatter {
                    format_options,
                    insert_final_newline,
                    ..
                } = self.config_resolver.resolve(&strategy)
                else {
                    return None;
                };

                let source_type = enable_jsx_source_type(source_type);
                let allocator = Allocator::new();
                let ret = Parser::new(&allocator, source_text, source_type)
                    .with_options(get_parse_options())
                    .parse();

                if !ret.errors.is_empty() {
                    return None;
                }

                let mut code = Formatter::new(&allocator, format_options).build(&ret.program);
                apply_insert_final_newline(&mut code, insert_final_newline);

                if code == *source_text {
                    return Some(vec![]);
                }

                Some(build_text_edits(source_text, &code))
            }
            FormatFileStrategy::OxfmtToml { .. } => None,
            FormatFileStrategy::ExternalFormatter { parser_name, .. } => {
                let Some(bridge) = &self.external_bridge else {
                    debug!("External formatter bridge not available for {}", path.display());
                    return None;
                };
                let Some(workspace_handle) = self.workspace_handle else {
                    debug!("External formatter workspace not available for {}", path.display());
                    return None;
                };

                let ResolvedOptions::ExternalFormatter {
                    external_options,
                    insert_final_newline,
                } = self.config_resolver.resolve(&strategy)
                else {
                    return None;
                };

                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                let code = match bridge.format_file(
                    workspace_handle,
                    &external_options,
                    parser_name,
                    file_name,
                    source_text,
                ) {
                    Ok(code) => code,
                    Err(err) => {
                        debug!("External formatter failed for {}: {err}", path.display());
                        return None;
                    }
                };

                let mut code = code;
                apply_insert_final_newline(&mut code, insert_final_newline);

                if code == *source_text {
                    return Some(vec![]);
                }

                Some(build_text_edits(source_text, &code))
            }
            FormatFileStrategy::ExternalFormatterPackageJson { parser_name, .. } => {
                let Some(bridge) = &self.external_bridge else {
                    debug!("External formatter bridge not available for {}", path.display());
                    return None;
                };
                let Some(workspace_handle) = self.workspace_handle else {
                    debug!("External formatter workspace not available for {}", path.display());
                    return None;
                };

                let ResolvedOptions::ExternalFormatterPackageJson {
                    external_options,
                    sort_package_json,
                    insert_final_newline,
                } = self.config_resolver.resolve(&strategy)
                else {
                    return None;
                };

                let source_text = if sort_package_json {
                    let options = SortOptions { sort_scripts: false, pretty: false };
                    match sort_package_json::sort_package_json_with_options(source_text, &options)
                    {
                        Ok(sorted) => Cow::Owned(sorted),
                        Err(err) => {
                            debug!("Failed to sort package.json {}: {err}", path.display());
                            return None;
                        }
                    }
                } else {
                    Cow::Borrowed(source_text)
                };

                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                let code = match bridge.format_file(
                    workspace_handle,
                    &external_options,
                    parser_name,
                    file_name,
                    source_text.as_ref(),
                ) {
                    Ok(code) => code,
                    Err(err) => {
                        debug!("External formatter failed for {}: {err}", path.display());
                        return None;
                    }
                };

                let mut code = code;
                apply_insert_final_newline(&mut code, insert_final_newline);

                if code == *source_text {
                    return Some(vec![]);
                }

                Some(build_text_edits(source_text.as_ref(), &code))
            }
        }
    }
}

impl ServerFormatter {
    pub fn new(
        config_resolver: ConfigResolver,
        gitignore_glob: Option<Gitignore>,
        external_bridge: Option<Arc<dyn ExternalFormatterBridge>>,
        workspace_handle: Option<WorkspaceHandle>,
    ) -> Self {
        Self { config_resolver, gitignore_glob, workspace_handle, external_bridge }
    }

    fn is_ignored(&self, path: &Path) -> bool {
        if let Some(glob) = &self.gitignore_glob {
            if !path.starts_with(glob.path()) {
                return false;
            }

            glob.matched_path_or_any_parents(path, path.is_dir()).is_ignore()
        } else {
            false
        }
    }
}

impl Drop for ServerFormatter {
    fn drop(&mut self) {
        let (Some(bridge), Some(handle)) = (&self.external_bridge, self.workspace_handle) else {
            return;
        };

        if let Err(err) = bridge.delete_workspace(handle) {
            debug!("Failed to delete external formatter workspace: {err}");
        }
    }
}

/// Returns the minimal text edit (start, end, replacement) to transform `source_text` into `formatted_text`
#[expect(clippy::cast_possible_truncation)]
fn compute_minimal_text_edit<'a>(
    source_text: &str,
    formatted_text: &'a str,
) -> (u32, u32, &'a str) {
    debug_assert!(source_text != formatted_text);

    // Find common prefix (byte offset)
    let mut prefix_byte = 0;
    for (a, b) in source_text.chars().zip(formatted_text.chars()) {
        if a == b {
            prefix_byte += a.len_utf8();
        } else {
            break;
        }
    }

    // Find common suffix (byte offset from end)
    let mut suffix_byte = 0;
    let src_bytes = source_text.as_bytes();
    let fmt_bytes = formatted_text.as_bytes();
    let src_len = src_bytes.len();
    let fmt_len = fmt_bytes.len();

    while suffix_byte < src_len - prefix_byte
        && suffix_byte < fmt_len - prefix_byte
        && src_bytes[src_len - 1 - suffix_byte] == fmt_bytes[fmt_len - 1 - suffix_byte]
    {
        suffix_byte += 1;
    }

    let start = prefix_byte as u32;
    let end = (src_len - suffix_byte) as u32;
    let replacement_start = prefix_byte;
    let replacement_end = fmt_len - suffix_byte;
    let replacement = &formatted_text[replacement_start..replacement_end];

    (start, end, replacement)
}

fn apply_insert_final_newline(code: &mut String, insert_final_newline: bool) {
    if !insert_final_newline {
        let trimmed_len = code.trim_end().len();
        code.truncate(trimmed_len);
    }
}

fn build_text_edits(source_text: &str, formatted_text: &str) -> Vec<TextEdit> {
    let (start, end, replacement) = compute_minimal_text_edit(source_text, formatted_text);
    let rope = Rope::from(source_text);
    let (start_line, start_character) = get_line_column(&rope, start, source_text);
    let (end_line, end_character) = get_line_column(&rope, end, source_text);

    vec![TextEdit::new(
        Range::new(
            Position::new(start_line, start_character),
            Position::new(end_line, end_character),
        ),
        replacement.to_string(),
    )]
}

// Almost the same as `oxfmt::walk::load_ignore_paths`, but does not handle custom ignore files.
fn load_ignore_paths(cwd: &Path) -> Vec<PathBuf> {
    [".gitignore", ".prettierignore"]
        .iter()
        .filter_map(|file_name| {
            let path = cwd.join(file_name);
            if path.exists() { Some(path) } else { None }
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests_builder {
    use crate::lsp::server_formatter::ServerFormatterBuilder;
    use oxc_language_server::{Capabilities, ToolBuilder};

    #[test]
    fn test_server_capabilities() {
        use tower_lsp_server::ls_types::{OneOf, ServerCapabilities};

        let builder = ServerFormatterBuilder::default();
        let mut capabilities = ServerCapabilities::default();

        builder.server_capabilities(&mut capabilities, &Capabilities::default());

        assert_eq!(capabilities.document_formatting_provider, Some(OneOf::Left(true)));
    }
}

#[cfg(test)]
mod test_watchers {
    // formatter file watcher-system does not depend on the actual file system,
    // so we can use a fake directory for testing.
    const FAKE_DIR: &str = "fixtures/formatter/watchers";

    mod init_watchers {
        use crate::lsp::{server_formatter::test_watchers::FAKE_DIR, tester::Tester};
        use serde_json::json;

        #[test]
        fn test_default_options() {
            let patterns = Tester::new(FAKE_DIR, json!({})).get_watcher_patterns();
            assert_eq!(patterns.len(), 2);
            assert_eq!(patterns[0], ".oxfmtrc.json");
            assert_eq!(patterns[1], ".oxfmtrc.jsonc");
        }

        #[test]
        fn test_formatter_custom_config_path() {
            let patterns = Tester::new(
                FAKE_DIR,
                json!({
                    "fmt.configPath": "configs/formatter.json"
                }),
            )
            .get_watcher_patterns();
            assert_eq!(patterns.len(), 1);
            assert_eq!(patterns[0], "configs/formatter.json");
        }

        #[test]
        fn test_empty_string_config_path() {
            let patterns = Tester::new(
                FAKE_DIR,
                json!({
                    "fmt.configPath": ""
                }),
            )
            .get_watcher_patterns();
            assert_eq!(patterns.len(), 2);
            assert_eq!(patterns[0], ".oxfmtrc.json");
            assert_eq!(patterns[1], ".oxfmtrc.jsonc");
        }
    }

    mod handle_configuration_change {
        use crate::lsp::{server_formatter::test_watchers::FAKE_DIR, tester::Tester};
        use oxc_language_server::ToolRestartChanges;
        use serde_json::json;

        #[test]
        fn test_no_change() {
            let ToolRestartChanges { watch_patterns, .. } =
                Tester::new(FAKE_DIR, json!({})).handle_configuration_change(json!({}));

            assert!(watch_patterns.is_none());
        }

        #[test]
        fn test_formatter_custom_config_path() {
            let ToolRestartChanges { watch_patterns, .. } = Tester::new(FAKE_DIR, json!({}))
                .handle_configuration_change(json!({
                    "fmt.configPath": "configs/formatter.json"
                }));

            assert!(watch_patterns.is_some());
            assert_eq!(watch_patterns.as_ref().unwrap().len(), 1);
            assert_eq!(watch_patterns.as_ref().unwrap()[0], "configs/formatter.json");
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::lsp::server_formatter::ServerFormatterBuilder;
    use super::compute_minimal_text_edit;
    use crate::lsp::tester::{Tester, get_file_uri};
    use oxc_language_server::Tool;

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn test_no_change() {
        let src = "abc";
        let formatted = "abc";
        compute_minimal_text_edit(src, formatted);
    }

    #[test]
    fn test_single_char_change() {
        let src = "abc";
        let formatted = "axc";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        // Only 'b' replaced by 'x'
        assert_eq!((start, end, replacement), (1, 2, "x"));
    }

    #[test]
    fn test_insert_char() {
        let src = "abc";
        let formatted = "abxc";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        // Insert 'x' after 'b'
        assert_eq!((start, end, replacement), (2, 2, "x"));
    }

    #[test]
    fn test_delete_char() {
        let src = "abc";
        let formatted = "ac";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        // Delete 'b'
        assert_eq!((start, end, replacement), (1, 2, ""));
    }

    #[test]
    fn test_replace_multiple_chars() {
        let src = "abcdef";
        let formatted = "abXYef";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        // Replace "cd" with "XY"
        assert_eq!((start, end, replacement), (2, 4, "XY"));
    }

    #[test]
    fn test_replace_multiple_chars_between_similars_complex() {
        let src = "aYabYb";
        let formatted = "aXabXb";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        assert_eq!((start, end, replacement), (1, 5, "XabX"));
    }

    #[test]
    fn test_unicode() {
        let src = "a😀b";
        let formatted = "a😃b";
        let (start, end, replacement) = compute_minimal_text_edit(src, formatted);
        // Replace 😀 with 😃
        assert_eq!((start, end, replacement), (1, 5, "😃"));
    }

    #[test]
    fn test_append() {
        let src = "a".repeat(100);
        let mut formatted = src.clone();
        formatted.push('b'); // Add a character at the end

        let (start, end, replacement) = compute_minimal_text_edit(&src, &formatted);
        assert_eq!((start, end, replacement), (100, 100, "b"));
    }

    #[test]
    fn test_prepend() {
        let src = "a".repeat(100);
        let mut formatted = String::from("b");
        formatted.push_str(&src); // Add a character at the start

        let (start, end, replacement) = compute_minimal_text_edit(&src, &formatted);
        assert_eq!((start, end, replacement), (0, 0, "b"));
    }

    #[test]
    fn test_formatter() {
        Tester::new(
            "test/fixtures/lsp/basic",
            json!({
                "fmt.experimental": true
            }),
        )
        .format_and_snapshot_single_file("basic.ts");
    }

    #[test]
    fn test_root_config_detection() {
        Tester::new(
            "test/fixtures/lsp/root_config",
            json!({
                "fmt.experimental": true
            }),
        )
        .format_and_snapshot_single_file("semicolons-as-needed.ts");
    }

    #[test]
    fn test_custom_config_path() {
        Tester::new(
            "test/fixtures/lsp/custom_config_path",
            json!({
                "fmt.experimental": true,
                "fmt.configPath": "./format.json",
            }),
        )
        .format_and_snapshot_single_file("semicolons-as-needed.ts");
    }

    #[test]
    fn test_ignore_files() {
        Tester::new(
            "test/fixtures/lsp/ignore-file",
            json!({
                "fmt.experimental": true
            }),
        )
        .format_and_snapshot_multiple_file(&["ignored.ts", "not-ignored.js"]);
    }

    #[test]
    fn test_ignore_pattern() {
        Tester::new(
            "test/fixtures/lsp/ignore-pattern",
            json!({
                "fmt.experimental": true
            }),
        )
        .format_and_snapshot_multiple_file(&["ignored.ts", "not-ignored.js"]);
    }

    #[test]
    fn test_prettier_only_without_bridge() {
        let root_uri = Tester::get_root_uri("test/fixtures/lsp/prettier_only");
        let formatter = ServerFormatterBuilder::build(&root_uri, json!({}), None);
        let files = ["sample.json", "sample.html"];
        for file in files {
            let uri = get_file_uri(&format!("test/fixtures/lsp/prettier_only/{file}"));
            let formatted = formatter.run_format(&uri, None);
            assert!(formatted.is_none(), "{file} should be skipped without bridge");
        }
    }
}
