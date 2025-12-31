import { notStrictEqual, strictEqual } from 'assert';
import {
  commands,
  Uri,
  window,
  workspace,
} from 'vscode';
import {
  activateExtension,
  fixturesWorkspaceUri,
  loadFixture,
  sleep,
} from './test-helpers';

suiteSetup(async () => {
  await activateExtension();
  await workspace.getConfiguration('editor').update('defaultFormatter', 'oxc.oxc-vscode');
  await workspace.saveAll();
});

teardown(async () => {
  await workspace.getConfiguration('oxc').update('fmt.configPath', undefined);
  await workspace.getConfiguration('editor').update('defaultFormatter', undefined);
  await workspace.saveAll();
});

suite('E2E Server Formatter', () => {
    // Skip tests if formatter tests are disabled
    if (process.env.SKIP_FORMATTER_TEST === 'true') {
      return;
    }

    test('formats code', async () => {
      await workspace.getConfiguration('editor').update('defaultFormatter', 'oxc.oxc-vscode');
      await workspace.saveAll();
      await loadFixture('formatting');

      await sleep(500);

      const fileUri = Uri.joinPath(fixturesWorkspaceUri(), 'fixtures', 'formatting.ts');

      const document = await workspace.openTextDocument(fileUri);
      await window.showTextDocument(document);
      await commands.executeCommand('editor.action.formatDocument');
      await workspace.saveAll();
      const content = await workspace.fs.readFile(fileUri);

      strictEqual(content.toString(), "class X {\n  foo() {\n    return 42;\n  }\n}\n");
    });

    test('formats code with `oxc.fmt.configPath`', async () => {
      await loadFixture('formatting_with_config');
      await workspace.getConfiguration('editor').update('defaultFormatter', 'oxc.oxc-vscode');
      await workspace.getConfiguration('oxc').update('fmt.configPath', './fixtures/formatter.json');
      await workspace.saveAll();

      const fileUri = Uri.joinPath(fixturesWorkspaceUri(), 'fixtures', 'formatting.ts');

      const document = await workspace.openTextDocument(fileUri);
      await window.showTextDocument(document);
      await sleep(500); // wait for the server to pick up the new config
      await commands.executeCommand('editor.action.formatDocument');
      await workspace.saveAll();
      const content = await workspace.fs.readFile(fileUri);

      strictEqual(content.toString(), "class X {\n  foo() {\n    return 42\n  }\n}\n");
    });

    test('formats prettier-only file types', async () => {
      await loadFixture('formatting_prettier');
      await workspace.getConfiguration('editor').update('defaultFormatter', 'oxc.oxc-vscode');
      await workspace.saveAll();

      await sleep(500);

      const expectedJson = "{ \"a\": 1, \"b\": [1, 2] }\n";
      const expectedJsonStringify = "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}\n";
      const expectedPackageJson = "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}\n\n";
      const expectedJson5 = "{ a: 1, b: [1, 2] }\n";
      const expectedCss = ".foo {\n  color: red;\n}\n";
      const expectedMarkdown = "# Title\n\n- a\n- b\n";
      const expectedHtml = "<div>\n  <span>Hi</span>\n  <span>There</span>\n</div>\n";
      const expectedAngular = "<div><span>Hi</span></div>\n";
      const expectedVue = "<template>\n  <div><span>Hi</span></div>\n</template>\n";
      const expectedMjml =
        "<mjml\n  ><mj-body\n    ><mj-section\n      ><mj-column><mj-text>Hello</mj-text></mj-column></mj-section\n    ></mj-body\n  ></mjml\n>\n";
      const expectedGraphql = "query {\n  user {\n    id\n    name\n  }\n}\n";
      const expectedYaml = "a: 1\nb:\n  - 1\n  - 2\n";
      const expectedHbs = "{{#if foo}}\n  <div>Hi</div>\n{{/if}}";

      const jsonFiles = [
        "prettier.json",
        "prettier.4DForm",
        "prettier.4DProject",
        "prettier.avsc",
        "prettier.geojson",
        "prettier.gltf",
        "prettier.har",
        "prettier.ice",
        "prettier.JSON-tmLanguage",
        "prettier.json.example",
        "prettier.mcmeta",
        "prettier.sarif",
        "prettier.tact",
        "prettier.tfstate",
        "prettier.tfstate.backup",
        "prettier.topojson",
        "prettier.webapp",
        "prettier.webmanifest",
        "prettier.yy",
        "prettier.yyp",
        ".all-contributorsrc",
        ".arcconfig",
        ".auto-changelog",
        ".c8rc",
        ".htmlhintrc",
        ".imgbotconfig",
        ".nycrc",
        ".tern-config",
        ".tern-project",
        ".watchmanconfig",
        ".babelrc",
        ".jscsrc",
        ".jshintrc",
        ".jslintrc",
        ".swcrc",
      ];

      const packageJsonFiles = ["package.json"];
      const jsonStringifyFiles = ["composer.json", "prettier.importmap"];
      const jsoncFiles = [
        "prettier.jsonc",
        "prettier.code-snippets",
        "prettier.code-workspace",
        "prettier.sublime-build",
        "prettier.sublime-color-scheme",
        "prettier.sublime-commands",
        "prettier.sublime-completions",
        "prettier.sublime-keymap",
        "prettier.sublime-macro",
        "prettier.sublime-menu",
        "prettier.sublime-mousemap",
        "prettier.sublime-project",
        "prettier.sublime-settings",
        "prettier.sublime-theme",
        "prettier.sublime-workspace",
        "prettier.sublime_metrics",
        "prettier.sublime_session",
      ];
      const json5Files = ["prettier.json5"];
      const cssFiles = [
        "prettier.css",
        "prettier.wxss",
        "prettier.pcss",
        "prettier.postcss",
        "prettier.less",
        "prettier.scss",
      ];
      const markdownFiles = [
        "prettier.md",
        "prettier.livemd",
        "prettier.markdown",
        "prettier.mdown",
        "prettier.mdwn",
        "prettier.mkd",
        "prettier.mkdn",
        "prettier.mkdown",
        "prettier.ronn",
        "prettier.scd",
        "prettier.workbook",
        "README",
        "contents.lr",
      ];
      const mdxFiles = ["prettier.mdx"];
      const htmlFiles = [
        "prettier.html",
        "prettier.htm",
        "prettier.hta",
        "prettier.inc",
        "prettier.xht",
        "prettier.xhtml",
      ];
      const angularFiles = ["prettier.component.html"];
      const vueFiles = ["prettier.vue"];
      const mjmlFiles = ["prettier.mjml"];
      const graphqlFiles = ["prettier.graphql", "prettier.gql", "prettier.graphqls"];
      const handlebarsFiles = ["prettier.handlebars", "prettier.hbs"];
      const yamlFiles = [
        "prettier.yml",
        "prettier.yaml",
        "prettier.mir",
        "prettier.reek",
        "prettier.rviz",
        "prettier.sublime-syntax",
        "prettier.syntax",
        "prettier.yaml-tmlanguage",
        ".clang-format",
        ".clang-tidy",
        ".clangd",
        ".gemrc",
        "CITATION.cff",
        "glide.lock",
        "pixi.lock",
        ".prettierrc",
        ".stylelintrc",
        ".lintstagedrc",
      ];

      const withExpected = (files: string[], expected: string) =>
        files.map((file): [string, string] => [file, expected]);

      const cases = [
        ...withExpected(jsonFiles, expectedJson),
        ...withExpected(packageJsonFiles, expectedPackageJson),
        ...withExpected(jsonStringifyFiles, expectedJsonStringify),
        ...withExpected(jsoncFiles, expectedJson),
        ...withExpected(json5Files, expectedJson5),
        ...withExpected(cssFiles, expectedCss),
        ...withExpected(markdownFiles, expectedMarkdown),
        ...withExpected(mdxFiles, expectedMarkdown),
        ...withExpected(htmlFiles, expectedHtml),
        ...withExpected(angularFiles, expectedAngular),
        ...withExpected(vueFiles, expectedVue),
        ...withExpected(mjmlFiles, expectedMjml),
        ...withExpected(graphqlFiles, expectedGraphql),
        ...withExpected(handlebarsFiles, expectedHbs),
        ...withExpected(yamlFiles, expectedYaml),
      ] satisfies Array<[string, string]>;

      // oxlint-disable eslint/no-await-in-loop -- VS Code formatting must be run sequentially per file.
      for (const [file, expected] of cases) {
        const fileUri = Uri.joinPath(fixturesWorkspaceUri(), 'fixtures', file);
        const original = (await workspace.fs.readFile(fileUri)).toString();
        notStrictEqual(original, expected, `${file} should differ before formatting`);
        const document = await workspace.openTextDocument(fileUri);
        await window.showTextDocument(document);
        await commands.executeCommand('editor.action.formatDocument');
        await workspace.saveAll();
        const content = await workspace.fs.readFile(fileUri);

        const actual = content.toString();
        strictEqual(actual, expected, `${file} should be formatted`);
      }
      // oxlint-enable eslint/no-await-in-loop
    });

});
