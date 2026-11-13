import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import ts from "typescript-eslint";

import svelteConfig from "./svelte.config.js";

export default ts.config(
  { ignores: ["dist/", "node_modules/", "src-tauri/", "docs/"] },
  js.configs.recommended,
  ...ts.configs.strict,
  ...svelte.configs.recommended,
  {
    languageOptions: { globals: { ...globals.browser } },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: [".svelte"],
        parser: ts.parser,
        svelteConfig,
      },
    },
  },
  {
    rules: {
      // `x == null` is the one loose comparison wanted: null or undefined.
      eqeqeq: ["error", "always", { null: "ignore" }],
      // TypeScript checks names itself.
      "no-undef": "off",
      // invoke<void>() is how a command without a result is typed.
      "@typescript-eslint/no-invalid-void-type": "off",
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    // Command-line scripts and configuration run in Node, and print.
    files: ["scripts/**", "*.config.js", "*.config.ts"],
    languageOptions: { globals: { ...globals.node } },
    rules: { "no-console": "off" },
  },
  {
    // The browser fake is a development aid, not shipped.
    files: ["src/dev/**"],
    rules: { "@typescript-eslint/no-non-null-assertion": "off" },
  },
);
