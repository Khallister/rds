# RDS (Rust Dependency Scanner)

🚀 A **fast**, **memory-efficient** dependency analyzer for JavaScript, TypeScript, and Vue projects, built with Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)

## ✨ Features

- 🔍 **Circular Dependency Detection** - Find problematic import cycles
- 🌳 **Dependency Tree Visualization** - See your project's import structure  
- ⚡ **High Performance** - Built with Rust for maximum speed
- 💾 **Memory Efficient** - Handles large codebases with minimal RAM usage
- 👁️ **Watch Mode** - Monitor files and re-analyze on changes (`--watch`)
- 🎨 **Beautiful Output** - Colorized, emoji-rich terminal output
- 📊 **Detailed Statistics** - Execution time, file counts, and progress tracking
- ⚡ **Fast Failure Mode** - `--take <COUNT>` flag for quick CI/CD checks
- 🔧 **CI/CD Ready** - Perfect for automation and git hooks
- 🎯 **Zero Configuration** - Works out of the box with sensible defaults

## 📋 Supported File Types

| Language/Framework | Extensions | Support Level |
|-------------------|------------|---------------|
| **JavaScript** | `.js`, `.mjs` | ✅ Full Support |
| **TypeScript** | `.ts`, `.tsx` | ✅ Full Support |
| **Vue.js** | `.vue` | ✅ SFC Support |
| **JSON** | `.json` | ✅ Module Resolution |
| **JSX/TSX** | `.jsx`, `.tsx` | ✅ React Components |

## 🚀 Installation

### Via NPM (Recommended)

```bash
# Global installation
npm install -g rds-analyzer

# Or use without installation
npx rds-analyzer --help

# Project-specific installation
npm install --save-dev rds-analyzer
```

### From Source

```bash
# Build from source
git clone https://github.com/yourusername/rds.git
cd rds
cargo build --release

# The binary will be available at ./target/release/rds
```

## 📖 Usage

### Basic Usage

```bash
# Check a single file for circular dependencies
rds src/index.js

# Analyze multiple files
rds src/index.js src/utils.js

# Analyze entire directory
rds src/

# Use glob patterns
rds "src/**/*.{js,ts,vue}"
```

### Advanced Usage

```bash
# Show dependency tree structure only
rds src/index.js --tree

# Show circular dependencies only
rds src/index.js --circular

# Limit circular dependency search (fast CI checks)
rds src/index.js --take 1 --throw

# Find at most 5 circular dependencies
rds src/index.js --circular --take 5

# Enable verbose logging
rds src/index.js --log

# Exit with error code for CI/CD
rds src/index.js --throw

# Output results to JSON file
rds src/index.js -o results.json

# Use with TypeScript configuration
rds src/index.ts --tsconfig ./tsconfig.json

# Enable file caching for faster analysis
rds src/ --cache

# Watch mode: monitor files and re-analyze on changes
rds src/ --watch

Note: `--cache` is enabled by default when running with `--watch`. Specify `--no-cache` to force cache-off behavior.

# Watch mode with caching (recommended for development)
rds src/ --watch --cache --circular

# Watch mode with tree view and take limit
rds src/ --watch --tree --take 1

# Filter specific file types
rds src/ --filter "js,ts,vue"

# Get version information
rds --version
```

## 🎯 Command Line Options

For comprehensive help with detailed descriptions of all options:
```bash
rds --help
```

### Core Options

| Flag | Description | Example |
|------|-------------|---------|
| `<FILES>...` | Files or directories to analyze | `rds src/` |
| `-o, --output <FILE>` | Output results to JSON file | `rds src/ -o deps.json` |
| `--context <DIR>` | Set working directory context | `rds src/ --context ./app` |
| `--version` | Display version information and exit | `rds --version` |
| `-h, --help` | Show help information and exit | `rds --help` |

### Analysis Options

| Flag | Description | Default |
|------|-------------|---------|
| `--circular` | Show circular dependencies only | ❌ Disabled* |
| `--tree` | Show dependency tree structure only | ❌ Disabled* |
| `--warning` | Show detailed warnings | ❌ Disabled |
| `--take <COUNT>` | Limit max circular dependencies to find | None (find all) |

***Default Behavior**: When neither `--tree` nor `--circular` is specified, both are shown.*

### Output Options

| Flag | Description | Example |
|------|-------------|---------|
| `--log` | Show verbose file analysis progress | `rds src/ --log` |
| `--progress <true\|false>` | Control progress bar display | `rds src/ --progress false` |

### CI/CD Options

| Flag | Description | Use Case |
|------|-------------|----------|
| `--throw` | Exit with code 1 if circular deps found | Git hooks, CI pipelines |
| `--exit-code <SPEC>` | Custom exit codes | `rds src/ --exit-code "circular:1"` |

### File Processing Options

| Flag | Description | Default Value |
|------|-------------|---------------|
| `--extensions <LIST>` | File extensions to analyze | `.ts,.tsx,.mjs,.js,.jsx,.json,.vue` |
| (removed) `--js <LIST>` | (deprecated) JavaScript-like extensions — use `--extensions` instead | `.ts,.tsx,.mjs,.js,.jsx` |
| `--filter <LIST>` | Filter by extension when scanning dirs | None |
| `--include <PATTERN>` | Include files matching pattern | `.*` |
| `--exclude <PATTERN>` | Exclude files matching pattern | `node_modules\|\.git\|...` |

### TypeScript Options

| Flag | Description | Example |
|------|-------------|---------|
| `--tsconfig <FILE>` | Use specific TypeScript config | `--tsconfig ./tsconfig.json` |
| (removed) `--transform` | (deprecated) Enable TypeScript transformations — currently not implemented; use transforms in your build pipeline | `rds src/ --transform` |

### Performance Options

| Flag | Description | Use Case |
|------|-------------|----------|
| `--cache` | Enable file caching for faster repeated analysis | Ideal for watch mode and development |
| `--no-cache` | Disable file caching (override default) | Force fresh analysis, debugging |
| `-W, --watch` | Monitor files for changes and re-analyze | Development workflow, live monitoring |

### Advanced Options

| Flag | Description | Options |
|------|-------------|---------|
| `--skip-dynamic-imports <TYPE>` | Skip dynamic imports in analysis | `tree`, `circular` |
| `--detect-unused-files-from <DIR>` | Detect unused files from directory | `rds src/ --detect-unused-files-from ./src` |

## 📊 Output Examples

### ✅ Clean Project (No Circular Dependencies)

```bash
🚀 Starting dependency analysis...
  [00:00:02] [████████████████████████████████████████] 150/150 Complete!
✨ Analysis complete! (2.43s)
📊 3 files processed, 150 total dependencies in tree
🧵 Analysis used 8 threads for parallel processing

🔄 Circular Dependencies
  ✅ Congratulations, no circular dependency was found in your project.
```

### ⚠️ Project with Issues

```bash
🚀 Starting dependency analysis...
  [00:00:01] [████████████████████████████████████████] 45/45 Complete!
✨ Analysis complete! (1.22s)
📊 2 files processed, 45 total dependencies in tree
🧵 Analysis used 8 threads for parallel processing

⚠️  Circular Dependencies
  1) src/utils/helper.js → src/components/Button.jsx → src/utils/helper.js
  2) src/store/index.js → src/store/modules/user.js → src/store/index.js
```

### 🌳 Dependency Tree View

```bash
🌳 Dependencies Tree
  - 0) src/index.js
      - 1) src/utils/api.js
          - 2) src/config/constants.js
          - 3) src/utils/helpers.js
      - 4) src/components/App.jsx
          - 5) src/components/Header.jsx
          - 1) src/utils/api.js
```

## 👁️ Watch Mode

RDS includes a powerful watch mode that monitors your files and automatically re-runs analysis when changes are detected:

```bash
# Start watch mode for a directory
rds src/ --watch

# Watch with specific analysis modes
rds src/ --watch --circular  # Focus on circular dependencies
rds src/ --watch --tree      # Show dependency tree on changes
rds src/ --watch --take 1    # Quick checks with early exit
```

### Watch Mode Features

- **Smart File Filtering**: Only monitors relevant file types (JS, TS, Vue, etc.)
- **Debounced Analysis**: Groups rapid changes to avoid excessive re-analysis
- **Directory Exclusion**: Automatically ignores `node_modules`, `.git`, and build directories
- **Recursive Monitoring**: Watches subdirectories automatically
- **Clean Output**: Compact analysis results optimized for watch mode
- **Graceful Exit**: Use `Ctrl+C` to stop watching

### Watch Mode Output

```bash
👁️ Starting watch mode...
📂 Watching: ./src
📂 Watching: ./components
💡 Press Ctrl+C to exit, or modify files to trigger analysis

📝 File changes detected, running analysis...
  📊 3 files, 12 deps (0.05s, 8 threads)
🔄 Circular Dependencies
  ✅ No circular dependencies found.
✅ Analysis complete, watching for changes...
```

### Development Workflow

Watch mode is perfect for:
- **Real-time Development**: Instant feedback as you code
- **Refactoring Sessions**: Monitor dependency changes during restructuring
- **Code Reviews**: Continuous validation of import structure
- **Team Development**: Shared watch sessions for collaborative debugging

## ⚡ Performance & Efficiency Features

### Fast Circular Dependency Detection

The `--take <COUNT>` flag allows you to limit the search and get results faster:

```bash
# Stop after finding 1 circular dependency (fastest)
rds src/ --take 1 --circular

# Find at most 3 circular dependencies  
rds src/ --take 3

# Example output with limit reached:
⚠️  Circular Dependencies
  1) src/utils/helper.js → src/components/Button.jsx
  At least 1 circular dependencies found (search limit reached)
```

**Use Cases:**
- **CI/CD Pipelines**: Fast failure with `--take 1 --throw`
- **Large Codebases**: Avoid long analysis times with reasonable limits
- **Development**: Quick feedback during refactoring

## 🔄 CI/CD Integration

### Git Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

# Fast check - fail immediately on first circular dependency
rds src/ --take 1 --throw
if [ $? -ne 0 ]; then
    echo "❌ Commit rejected: Circular dependencies detected!"
    exit 1
fi
```

### GitHub Actions

```yaml
name: Dependency Check
on: [push, pull_request]
jobs:
  check-deps:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install RDS
        run: |
          # Build or download rds binary
          cargo build --release
      - name: Check for circular dependencies
        run: |
          # Fast check for CI - fail after first circular dependency found
          ./target/release/rds src/ --take 1 --throw
```

### Package.json Scripts

```json
{
  "scripts": {
    "deps:check": "rds src/ --take 1 --throw",
    "deps:check-all": "rds src/ --throw", 
    "deps:tree": "rds src/ --tree",
    "deps:analyze": "rds src/ --tree --warning -o deps.json"
  }
}
```

## ⚙️ Configuration

### TypeScript Configuration

RDS automatically reads your `tsconfig.json` for path mappings and module resolution:

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"],
      "@/components/*": ["src/components/*"]
    }
  }
}
```

### Custom Exclusions

```bash
# Exclude specific patterns
rds src/ --exclude "node_modules|\.git|\.svn|coverage|dist|build"

# Include only specific patterns  
rds src/ --include "src.*\.(js|ts|vue)$"
```

## 📈 Performance

RDS is designed for performance and can handle large codebases efficiently:

- **Multi-Threading**: Utilizes all CPU cores with parallel processing (shows thread count in output)
- **Memory Usage**: ~10-50MB for typical projects (1000+ files)
- **Speed**: Analyzes 1000+ files in under 5 seconds with parallel processing
- **Scalability**: Tested with projects containing 10,000+ files
- **Async I/O**: Non-blocking file operations for maximum throughput

### Performance Features
- **Parallel File Processing**: Multiple files analyzed simultaneously
- **Efficient Memory Management**: Rust's zero-cost abstractions
- **Optimized Algorithms**: Fast circular dependency detection
- **Progress Tracking**: Real-time analysis progress with timing information

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

This project was inspired by and builds upon the excellent work of:

### 🌟 Original DPDM Package
**RDS** is a Rust-based reimplementation inspired by the fantastic [**dpdm**](https://github.com/acrazing/dpdm) package created by [acrazing](https://github.com/acrazing).

- **Original Repository**: https://github.com/acrazing/dpdm
- **What we learned**: TypeScript compiler API usage, dependency resolution patterns, and CLI design
- **Why Rust**: To provide a more memory-efficient alternative while maintaining feature parity

### 🛠️ Technology Stack
- Built with the amazing **Rust ecosystem**
- **TypeScript Compiler API insights** from the original implementation
- **Tokio** for async file processing
- **Clap** for CLI argument parsing
- **SWC** for JavaScript/TypeScript parsing

### 🎯 Design Philosophy
While RDS maintains full feature compatibility with the original dpdm, it focuses on:
- **Memory efficiency** through Rust's ownership system
- **Performance** via parallel processing and optimized algorithms  
- **Enhanced UX** with progress bars, colors, and better error messages
- **Cross-platform distribution** through npm packaging

**Huge thanks to the original dpdm team for pioneering dependency analysis tooling!** 🙌

---

**Made with ❤️ and Rust** 🦀
