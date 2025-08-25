const { spawn, spawnSync } = require("child_process");
const path = require("path");
const fs = require("fs");

function chooseExecutable() {
  return "rds";
}
function ensureArray(v) {
  if (!v) return [];
  return Array.isArray(v) ? v : [v];
}

module.exports = function rdsWatchPlugin(options = {}) {
  let child = null;
  let serverClosed = false;
  let stopPromise = null;
  const exe = options.execPath || chooseExecutable();
  const args = ensureArray(options.args || ["--watch"]);
  const cwd = options.cwd || process.cwd();
  const useShell =
    typeof options.useShell === "boolean"
      ? options.useShell
      : process.platform === "win32";

  const stopOnServerClose =
    typeof options.stopOnServerClose === "boolean"
      ? options.stopOnServerClose
      : true;
  const restartOnHotUpdate =
    typeof options.restartOnHotUpdate === "boolean"
      ? options.restartOnHotUpdate
      : false;

  function quoteIfNeeded(s) {
    if (!useShell || typeof s !== "string") return s;
    if (/[\s&|<>^]/.test(s)) return `"${s.replace(/"/g, '\\"')}"`;
    return s;
  }
  function resolveCommand(cmd) {
    if (path.isAbsolute(cmd)) return cmd;
    if (cmd.includes(path.sep) || cmd.includes("/")) return path.join(cwd, cmd);
    return cmd;
  }

  function findExecutableOnPath(cmd) {
    try {
      if (process.platform === "win32") {
        const r = spawnSync("where", [cmd], { encoding: "utf8" });
        if (r.status === 0 && r.stdout)
          return r.stdout.split(/\r?\n/)[0].trim();
      } else {
        const r = spawnSync("which", [cmd], { encoding: "utf8" });
        if (r.status === 0 && r.stdout)
          return r.stdout.split(/\r?\n/)[0].trim();
      }
    } catch {}
    return null;
  }

  function followShimIfNeeded(found) {
    try {
      if (!found) return null;
      if (!fs.existsSync(found)) return found;
      const text = fs.readFileSync(found, "utf8");
      const m = text.match(/exec\s+["']?\$basedir[\\\/]+([^"'\s]+rds\.exe)["']?/i);
      if (m && m[1]) {
        const rel = m[1].replace(/\//g, path.sep);
        const candidate = path.resolve(path.dirname(found), rel);
        if (fs.existsSync(candidate)) return candidate;
      }
    } catch {}
    return found;
  }

  function attachHandlers(c, attemptedExe, tried) {
    c.on("exit", (code) => {
      child = null;
      if (code && code !== 0)
        console.warn("[vite-plugin-rds-watch] rds exited with code", code);
    });
    c.on("close", () => {
      child = null;
    });
    c.on("error", (err) => {
      if (
        err &&
        err.code === "ENOENT" &&
        attemptedExe === "rds" &&
        !tried.npx
      ) {
        tried.npx = true;
        child = spawn("npx", ["rds", ...args], {
          cwd,
          stdio: "inherit",
          shell: useShell,
        });
        attachHandlers(child, "npx", tried);
        return;
      }
      if (
        err &&
        err.code === "ENOENT" &&
        attemptedExe === "npx" &&
        !tried.npm
      ) {
        tried.npm = true;
        child = spawn("npm", ["exec", "--", "rds", ...args], {
          cwd,
          stdio: "inherit",
          shell: useShell,
        });
        attachHandlers(child, "npm", tried);
        return;
      }
      console.error(
        "[vite-plugin-rds-watch] failed to start process:",
        err && err.message
      );
    });
  }

  async function start() {
    if (serverClosed) return;
    if (child) return;

    try {
      await stopChild();
    } catch {}

    if (serverClosed) return;
    if (child) return;

    const spawnArgs = args.slice();
    const hasNonFlag = spawnArgs.some(
      (a) => typeof a === "string" && !a.startsWith("-")
    );
    if (!hasNonFlag) spawnArgs.push(".");
    const hasExclude = spawnArgs.some(
      (a) =>
        typeof a === "string" &&
        (a === "--exclude" || a.startsWith("--exclude="))
    );
    if (!hasExclude)
      spawnArgs.push("--exclude", "node_modules|.git|.vite|dist|build|.cache");
    const spawnArgsForShell = spawnArgs.map(quoteIfNeeded);
    let resolved = resolveCommand(exe);
    const spawnOptions = { cwd, stdio: "inherit", shell: useShell };
    // On Windows prefer to spawn the real rds.exe directly (no shell) when available.
    if (process.platform === "win32" && useShell) {
      const found = findExecutableOnPath(exe);
      if (found) {
        const real = followShimIfNeeded(found);
        if (real) resolved = real;
        spawnOptions.shell = false;
      }
    }
    const argsToPass = spawnOptions.shell ? spawnArgsForShell : spawnArgs;
  child = spawn(resolved, argsToPass, spawnOptions);
    attachHandlers(child, "rds", { npx: false, npm: false });
  }

  function stopChild() {
    if (!child) return Promise.resolve();
  const pid = child.pid;
    return new Promise((resolve) => {
      try {
        child.kill("SIGTERM");
      } catch {}
      if (process.platform === "win32") {
        try {
          spawn("taskkill", ["/PID", String(pid), "/T", "/F"], {
            stdio: "ignore",
            shell: true,
          });
        } catch {}
      } else {
        try {
          process.kill(pid, 0);
        } catch {
          child = null;
          return resolve();
        }
        try {
          process.kill(pid, "SIGKILL");
        } catch {}
      }
      const maxAttempts = 20;
      let attempts = 0;
      const iv = setInterval(() => {
        attempts += 1;
        let alive = true;
        try {
          process.kill(pid, 0);
        } catch {
          alive = false;
        }
        if (!alive || attempts >= maxAttempts) {
          clearInterval(iv);
          // If still alive on Windows, try image-name kill as a last resort
          if (alive && process.platform === "win32") {
            try {
              spawn("taskkill", ["/F", "/IM", "rds.exe", "/T"], {
                stdio: "ignore",
                shell: true,
              });
            } catch {}
          }
          child = null;
          return resolve();
        }
      }, 200);
    });
  }

  function stop() {
    serverClosed = true;
    if (stopPromise) {
      return stopPromise;
    }
    stopPromise = (async () => {
      try {
        await stopChild();
      } finally {
        stopPromise = null;
      }
    })();
    return stopPromise;
  }

  return {
    name: "vite-plugin-rds-watch",
    configureServer(server) {
      start();
      if (stopOnServerClose)
        server.httpServer && server.httpServer.on("close", () => stop());
    },
    handleHotUpdate(ctx) {
      const file = ctx.file || "";
      if (
        restartOnHotUpdate &&
        (file.endsWith("rds.config.toml") || file.includes("rds"))
      ) {
        stop();
        start();
      }
      return [];
    },
    closeBundle() {
      stop();
    },
  };
};
