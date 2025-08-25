# vite-plugin-rds-watch

Vite plugin that runs the `rds` executable (or `rds.exe` on Windows) in watch mode together with the dev server. Useful for projects that want to run platform-dependent tooling while developing frontend code.

Usage (in consuming project):

1. Install the plugin package (from this repo or published):

```bash
npm install --save-dev ./path/to/rds/plugin
# or
npm install --save-dev vite-plugin-rds-watch
```

2. Add to your `vite.config.js`:

```js
import { defineConfig } from 'vite'
import rdsWatch from 'vite-plugin-rds-watch'

export default defineConfig({
  plugins: [rdsWatch({ execPath: 'rds', args: ['--watch', '--tree'] })]
})
```

Options:
- `execPath` (string): path or name of the executable to run. Defaults to `rds` or `rds.exe` on Windows.
- `args` (array): arguments passed to the executable (default `['--watch']`).
- `cwd` (string): working directory for the spawned process. Defaults to Vite's process cwd.

Behavior:
- The plugin starts the executable when the dev server starts and stops it when the server closes.
- On hot updates that touch files with `rds` in the name or `rds.config.toml`, the plugin restarts the process to pick up config changes.
