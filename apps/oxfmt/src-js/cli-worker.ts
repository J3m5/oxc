// `oxfmt` CLI - Worker Thread Entry Point

// Re-exports core functions for use in `worker_threads`
export { createWorkspace, deleteWorkspace, formatEmbeddedCode, formatFile } from "./libs/prettier";
