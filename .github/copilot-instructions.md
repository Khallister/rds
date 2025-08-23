# Copilot Instructions for RDS (Rust Dependency Scanner)

## Project Overview

- **RDS** is a high-performance dependency analyzer for JavaScript, TypeScript, and Vue projects, implemented in Rust.
- The tool detects circular dependencies, visualizes dependency trees, and supports advanced workflows like watch mode and CI/CD integration.
- RDS is inspired by the [dpdm](https://github.com/acrazing/dpdm) package but re-architected for memory efficiency and speed.

## Refactored Architecture (Post-v0.4.0)

The codebase has been refactored into a modular, maintainable structure:

### Core Modules

- **`src/main.rs`**: Minimal entry point - only handles CLI parsing validation, thread pool configuration, and delegation
- **`src/cli.rs`**: Complete CLI argument definitions, parsing, and validation logic
- **`src/analysis_runner.rs`**: Orchestrates single-run analysis workflow, progress reporting, and result output
- **`src/watch.rs`**: Handles watch mode functionality with file monitoring and incremental analysis
- **`src/filesystem.rs`**: File discovery, directory scanning, and filtering operations
- **`src/utils.rs`**: Utility functions organized by purpose:
  - `config::create_parse_options_from_cli()` - Convert CLI args to parse options
  - `threading::configure_thread_pool()` - Thread pool management
  - `exit_codes::handle_exit_codes()` - Custom exit code handling

### Analysis Modules

- **`src/analyzer/`**: Core dependency analysis logic (unchanged)
  - `tree.rs`: Builds dependency trees with parallel processing
  - `circular.rs`: Detects circular dependencies
  - `unused.rs`: (Stub) unused file detection
- **`src/parser/`**: Language-specific parsing (unchanged)
- **`src/output/`**: Result formatting (unchanged)
- **`src/types/`**: Shared type definitions (unchanged)

## Key Workflows

### Build & Development
```bash
cargo build --release        # Production build
cargo check                  # Fast compilation check
cargo test                   # Run tests
```

### Running Analysis
```bash
# Basic usage
rds src/ --tree --circular

# Watch mode with caching
rds src/ --watch --cache

# CI/CD integration (fast failure)
rds src/ --take 1 --throw
```

### Testing Strategy
Each module is now independently testable:
- **CLI**: Test argument validation in `src/cli.rs`
- **File System**: Test file discovery in `src/filesystem.rs`  
- **Analysis**: Test workflow orchestration in `src/analysis_runner.rs`
- **Watch Mode**: Test file monitoring in `src/watch.rs`

## Project-Specific Patterns

### Dependency Injection
- Pass dependencies (analyzers, config) as parameters to functions
- Use `ParseOptions` struct for configuration propagation
- CLI validation happens early in `main()`, then options are transformed

### Error Handling
- Use `anyhow::Result` for most functions
- Validation errors are handled at CLI level and exit early
- Analysis errors are propagated up through the call stack

### Module Organization
- **Single Responsibility**: Each module handles one aspect (CLI, filesystem, analysis, watch)
- **Separation of Concerns**: Output logic is isolated from analysis logic
- **Testability**: Pure functions where possible, dependency injection for side effects

## Extension Points

### Adding New Output Formats
Add new output implementations in `src/output/` and call from `AnalysisRunner::display_analysis_results()`

### Adding New File Types
Extend `FileSystem::should_include_file()` and add parsing logic to `src/parser/`

### Adding New CLI Options
1. Add to `Cli` struct in `src/cli.rs`
2. Update validation in `Cli::validate()`  
3. Handle in `utils::config::create_parse_options_from_cli()`

## Key Integration Points

- **NPM Package**: Binary is packaged as `rds-analyzer` npm package
- **CLI Interface**: All functionality exposed through `src/cli.rs` definitions
- **JSON Output**: Machine-readable results via `-o output.json`
- **Exit Codes**: Configurable via `--exit-code` for CI/CD integration

## Development Guidelines

- **Keep `main.rs` minimal** - delegate to specialized modules
- **Test at module boundaries** - each module should be independently testable  
- **Use `cargo check`** for fast feedback during development
- **Preserve backwards compatibility** in CLI interface
- **Document public APIs** with rustdoc comments

## References

- **Entry Point**: `src/main.rs` - shows the overall application flow
- **CLI Definition**: `src/cli.rs` - complete argument structure
- **Analysis Flow**: `src/analysis_runner.rs` - understand the analysis pipeline
- **Watch Mode**: `src/watch.rs` - file monitoring and incremental analysis
- **File Operations**: `src/filesystem.rs` - file discovery patterns
