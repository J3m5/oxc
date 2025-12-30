import { createRequire } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";
import type { Options } from "prettier";

// Lazy load Prettier
//
// NOTE: In the past, statically importing caused issues with `oxfmt --lsp` not starting.
// However, this issue has not been observed recently, possibly due to changes in the bundling configuration.
// Anyway, we keep lazy loading for now to minimize initial load time.
let prettierCache: typeof import("prettier");

type Workspace = {
  id: number;
  root: string;
  prettier: typeof import("prettier");
};

const workspaces = new Map<number, Workspace>();
let nextWorkspaceId = 1;

async function loadPrettierDefault(): Promise<typeof import("prettier")> {
  if (!prettierCache) {
    prettierCache = await import("prettier");
  }
  return prettierCache;
}

async function loadPrettierForRoot(root: string): Promise<typeof import("prettier")> {
  try {
    const requireFromRoot = createRequire(path.join(root, "package.json"));
    const prettierPath = requireFromRoot.resolve("prettier");
    return import(pathToFileURL(prettierPath).href);
  } catch {
    return loadPrettierDefault();
  }
}

async function getWorkspacePrettier(workspaceId?: number) {
  if (workspaceId && workspaceId > 0) {
    const workspace = workspaces.get(workspaceId);
    if (workspace) return workspace;
  }
  return {
    id: 0,
    root: "",
    prettier: await loadPrettierDefault(),
  };
}

export async function createWorkspace(directory: string): Promise<number> {
  if (!directory) {
    throw new TypeError("`directory` must be a non-empty string");
  }

  const prettier = await loadPrettierForRoot(directory);
  const id = nextWorkspaceId++;
  workspaces.set(id, { id, root: directory, prettier });
  return id;
}

export async function deleteWorkspace(workspaceId: number): Promise<void> {
  workspaces.delete(workspaceId);
}

/**
 * TODO: Plugins support
 * - Read `plugins` field
 * - Load plugins dynamically and parse `languages` field
 * - Map file extensions and filenames to Prettier parsers
 *
 * @returns Array of loaded plugin's `languages` info
 */
export async function resolvePlugins(): Promise<string[]> {
  return [];
}

// ---

const TAG_TO_PARSER: Record<string, string> = {
  // CSS
  css: "css",
  styled: "css",
  // GraphQL
  gql: "graphql",
  graphql: "graphql",
  // HTML
  html: "html",
  // Markdown
  md: "markdown",
  markdown: "markdown",
};

export type FormatEmbeddedCodeParam = {
  code: string;
  tagName: string;
  options: Options;
};

/**
 * Format xxx-in-js code snippets
 *
 * @returns Formatted code snippet
 * TODO: In the future, this should return `Doc` instead of string,
 * otherwise, we cannot calculate `printWidth` correctly.
 */
export async function formatEmbeddedCode({
  code,
  tagName,
  options,
}: FormatEmbeddedCodeParam): Promise<string> {
  // TODO: This should be resolved in Rust side
  const parserName = TAG_TO_PARSER[tagName];

  // Unknown tag, return original code
  if (!parserName) return code;

  if (!prettierCache) {
    prettierCache = await import("prettier");
  }

  // SAFETY: `options` is created in Rust side, so it's safe to mutate here
  options.parser = parserName;
  return prettierCache
    .format(code, options)
    .then((formatted) => formatted.trimEnd())
    .catch(() => code);
}

// ---

export type FormatFileParam = {
  workspaceId?: number;
  code: string;
  parserName: string;
  fileName: string;
  options: Options;
};

/**
 * Format non-js file
 *
 * @returns Formatted code
 */
export async function formatFile({
  workspaceId,
  code,
  parserName,
  fileName,
  options,
}: FormatFileParam): Promise<string> {
  const workspace = await getWorkspacePrettier(workspaceId);

  // SAFETY: `options` is created in Rust side, so it's safe to mutate here
  // We specify `parser` to skip parser inference for performance
  options.parser = parserName;
  // But some plugins rely on `filepath`, so we set it too
  options.filepath = fileName;
  return workspace.prettier.format(code, options);
}
