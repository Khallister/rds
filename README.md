# RDS — Rust Dependency Scanner

A compact, developer-focused dependency analyzer for JavaScript, TypeScript, and Vue projects.

[MIT License](LICENSE)

## What it does
- Detect circular dependencies (default output).
- Show full dependency tree when explicitly requested with `--tree`.
- Watch mode with smart caching (`--watch`).

## Quick examples
- Analyze a directory (circulars only):

  `rds src/`

- Show dependency tree (opt-in):

  `rds src/ --tree`

- Fast CI check (fail on first circular):

  `rds src/ --take 1 --throw`

- Run watch mode but first perform a full scan to ensure there are no pre-existing circulars:

  `rds src/ --watch --pre-scan`

## Core flags (condensed)
- `<FILES>...` — files or directories to analyze (required)
- `--tree` — show dependency tree (opt-in)
- `--circular` — show circular dependencies only
- `--take <N>` — stop after finding N circulars
- `--throw` — exit with code 1 if circulars found
- `--watch` / `-W` — watch files and re-run analysis
- `--cache` / `--no-cache` — control file caching (default: enabled for `--watch`)
- `--log` — enable verbose logging
- `-o, --output <FILE>` — write results to JSON
- `--resolve-concurrency <N>` — limit concurrent module resolution tasks (defaults to automatic tuning). Use this to control IO/concurrency during large analyses.
- `--pre-scan` — when used with `--watch`, run a full initial analysis (circulars/tree) before starting watch mode; useful to ensure no existing circular dependencies are present since watch only re-scans changed files.

Note: If you use `--watch`, the `--throw` option is ignored. Watch mode is long-running and intended to keep the process alive while monitoring changes, so the tool will not exit the process on circulars when running under `--watch`. If you need a failing CI-style run, use `--take`/`--throw` without `--watch` or use `--pre-scan` with `--watch` and run a separate short-lived check.

## Developer notes
- CLI implementation: `src/cli/mod.rs` (current flags & defaults)
- Parser registry: `src/parser/mod.rs` (runtime uses `get_parser_for_extension`)
- Tree builder and analyzer: `src/analyzer/tree` and `src/analyzer`
- Output backends: `src/output`

Testing & development
- Run tests: `cargo test --lib`
- Build release: `cargo build --release`

## Acknowledgments

Inspired by the dpdm project: https://github.com/acrazing/dpdm

---

Made with ❤️ and Rust 🦀

## Installation

- From npm (recommended):

  `npm install -g rds-analyzer`

- Use without installing:

  `npx rds-analyzer --help`

- From source:

  ```bash
  git clone https://github.com/Khallister/rds.git
  cd rds

  # Build the native binary
  cargo build --release

  # Optionally create the npm package tarball (from repo root) and install locally
  # `npm pack` will generate a file like `rds-analyzer-<version>.tgz` based on package.json
  npm pack

  # Install the package locally (global) for testing the CLI runner
  npm install -g ./rds-analyzer-*.tgz

  # Or install as a dev dependency in a project
  npm install --save-dev ./rds-analyzer-*.tgz

  # binary: ./target/release/rds (packaging will include the native binary in the npm bundle)
  ```

## Watch mode & caching

- `--watch` enables file monitoring and re-analysis on changes.
- By default, caching is enabled when using `--watch` to make incremental runs fast. Use `--no-cache` to disable caching for debugging.

## Packaging & publishing

- The npm package bundles a small JS runner and the native binary in `bin/`.
- Use the included build scripts to cross-compile for multiple platforms when preparing releases.

## Contributing

- Add focused unit tests under `src/*/tests.rs` when changing behavior.

## License

- MIT

