## Copilot instructions — RDS (Rust Dependency Scanner)

Purpose: minimal, actionable guidance to make automated coding agents productive in this repo.

Quick commands
- Build (release): cargo build --release
- Fast check: cargo check
- Tests (local): cargo test --lib
- Tests (CI/full): cargo test --all-features
- Run CLI: cargo run -- <paths> --tree --circular or use bin/rds.exe on Windows

Source layout (authoritative)
- `src/lib.rs` — library exports and re-exports (top-level API used in tests)
- `src/main.rs` — binary startup: parse CLI, configure thread pool, register builtin parsers, then delegate to `analysis_runner` or `watch`.
- `src/cli/` — clap-based CLI; `Cli::parse_args()` sanitizes args for test harness.
- `src/analysis_runner/` — orchestrates analysis runs and printing (tree/circular/JSON).
- `src/watch/` — watch-mode event loop and debounce logic.
- `src/filesystem/` — file discovery, include/exclude rules, glob scanning.
- `src/analyzer/` — analysis core: tree building and circular detection.
- `src/parser/` — parser registry, parser plugins, resolver (see `parser/mod.rs`, `parser/plugins/`, `parser/resolver.rs`).
- `src/output/` — Console and JSON output backends.
- `src/utils/`, `src/cache/`, `src/types/` — helpers, caching, and shared types.

Notable code patterns
- Parser registry: `parser/mod.rs` defines an object-safe `Parser` trait (dyn Parser) and a runtime registry; use `register_parser()` to add parsers (main registers JavaScriptParser and VueParser at startup).
- JavaScript parsing: `parser/plugins/javascript.rs` uses regexes and a `remove_comments()` pre-pass that preserves module-specifier strings and scrubs import-like keywords inside non-specifier strings. Search for `remove_comments` if parsing false-positives appear.
- Module resolution: `parser/resolver.rs` implements Node-like probing (`append_suffix`), directory/index resolution, and package.json `module`/`main` handling; TS `paths` aliasing is supported — tests in `parser/resolver/tests.rs` encode expected behaviors.
- DI/testability: functions accept `ParseOptions` and explicit dependencies. Follow this pattern to keep features testable.

Developer workflows & gotchas
- Husky hooks:
  - `.husky/pre-commit` runs `cargo fmt --all` then `lint-staged` and restages formatting changes with `git add -u`.
  - `.husky/pre-push` runs `cargo test --lib` and aborts pushes on failure.
- lint-staged caveat: it appends staged file paths to commands. Do not put `cargo test` in lint-staged (we run tests in pre-push instead).
- Tests: unit tests always live in `src/*/tests.rs` (not in mod.rs files) and use `tempfile::tempdir()` and platform-aware assertions (`#[cfg(unix)]` when needed).

Where to look first (examples)
- Add parser: read `src/parser/plugins/javascript.rs`, then `src/parser/mod.rs` for registration and `src/parser/tests.rs` for runtime tests.
- Change output: inspect `src/output/console.rs`, `src/output/json.rs`, and how `AnalysisRunner::display_analysis_results()` calls them.
- Fix resolution: `src/parser/resolver.rs` and its tests under `src/parser/resolver/tests.rs`.

Edit-safety checklist
1. Run `cargo test --lib` locally before committing; CI runs `cargo test --all-features`.
2. Preserve the DI pattern: accept `ParseOptions` and explicit dependencies rather than globals.
3. Add focused unit tests under the same `src/*/tests.rs` module (use `tempfile` for fixtures).
4. If you add CLI flags, update `src/cli/` and `utils::config::create_parse_options_from_cli()` and document changes in README.

If anything here is unclear or you want focused examples (parsers, resolver, hooks), tell me which area to expand and I will iterate.
```
