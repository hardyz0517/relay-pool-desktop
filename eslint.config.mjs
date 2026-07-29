import fs from "node:fs";
import js from "@eslint/js";
import tseslint from "typescript-eslint";

const featureNames = fs
  .readdirSync(new URL("./src/features/", import.meta.url), { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

const featureBoundaries = featureNames.map((feature) => ({
  files: [`src/features/${feature}/**/*.{ts,tsx}`],
  rules: {
    "no-restricted-imports": [
        "warn",
      {
        patterns: featureNames
          .filter((candidate) => candidate !== feature)
          .map((candidate) => ({
            group: [`@/features/${candidate}/*`, `@/features/${candidate}/**`],
            message: `Import feature '${candidate}' through its public index; resolved graph checks prevent barrel bypasses.`,
          })),
      },
    ],
  },
}));

export default tseslint.config(
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "output/**",
      "src-tauri/target/**",
      "src-tauri/gen/**",
      "scripts/architecture/fixtures/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["scripts/architecture/**/*.mjs"],
    languageOptions: {
      globals: {
        Buffer: "readonly",
        URL: "readonly",
        console: "readonly",
        process: "readonly",
      },
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      "@typescript-eslint/no-unused-vars": "warn",
      "prefer-const": "warn",
    },
  },
  ...featureBoundaries,
);
