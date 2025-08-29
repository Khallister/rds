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
  let exe = options.execPath || chooseExecutable();
  let args = ensureArray(options.args || ["--watch"]);
  const cwd = options.cwd || process.cwd();
  const pidFile = path.join(cwd, ".rds-plugin.pid");
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

  function matchShimPatterns(text, found) {
    const patterns = [
      /exec\s+["']?\$basedir[\\/]+([^"'\s]*rds(?:\.exe)?)["']?/i,
      /require\(['"].*bin[\\/]([^"']*rds(?:\.exe)?)["']\)/i,
      /path\.join\(.*__dirname.*[,\s]*['"]?\.\.[\\/]bin[\\/]([^'")]+)['"]?\)/i,
    ];
    for (const pat of patterns) {
      const m = text.match(pat);
      if (m && m[1]) {
        const rel = m[1].replace(/\//g, path.sep);
        const candidate = path.resolve(path.dirname(found), rel);
        if (fs.existsSync(candidate)) return candidate;
      }
    }
    return null;
  }

  function findNearbyBinCandidate(found) {
    const tryDirs = [
      path.dirname(found),
      path.resolve(path.dirname(found), ".."),
      path.resolve(path.dirname(found), "..", ".."),
    ];
    const names = ["rds.exe", "rds"];
    for (const d of tryDirs) {
      for (const n of names) {
        const p = path.join(d, "bin", n);
        if (fs.existsSync(p)) return p;
      }
    }
    return null;
  }

  function followShimIfNeeded(found) {
    try {
      if (!found) return null;
      if (!fs.existsSync(found)) return found;
      const real = fs.realpathSync(found);
      if (real && real !== found && fs.existsSync(real)) return real;
      const text = fs.readFileSync(found, "utf8");
      const shimCandidate = matchShimPatterns(text, found);
      if (shimCandidate) return shimCandidate;
      const binCandidate = findNearbyBinCandidate(found);
      if (binCandidate) return binCandidate;
    } catch {}
    return found;
  }

  function attachHandlers(c, attemptedExe, tried) {
    c.on("exit", (code) => {
      child = null;
      try {
        if (fs.existsSync(pidFile)) fs.unlinkSync(pidFile);
      } catch {}
      if (code && code !== 0)
        console.warn("[vite-plugin-rds-watch] rds exited with code", code);
    });
    c.on("close", () => {
      child = null;
      try {
        if (fs.existsSync(pidFile)) fs.unlinkSync(pidFile);
      } catch {}
    });
    c.on("error", (err) => {
      if (
        err &&
        err.code === "ENOENT" &&
        attemptedExe === "rds" &&
        !tried.npx
      ) {
        tried.npx = true;
  setChild(spawn("npx", ["rds", ...args], { cwd, stdio: "inherit", shell: useShell }), "npx", tried);
        return;
      }
      if (
        err &&
        err.code === "ENOENT" &&
        attemptedExe === "npx" &&
        !tried.npm
      ) {
        tried.npm = true;
  setChild(spawn("npm", ["exec", "--", "rds", ...args], { cwd, stdio: "inherit", shell: useShell }), "npm", tried);
        return;
      }
      console.error(
        "[vite-plugin-rds-watch] failed to start process:",
        err && err.message
      );
    });
  }

  function setChild(c, attemptedExe, tried) {
    child = c;
    try { if (child && child.pid) fs.writeFileSync(pidFile, String(child.pid), { encoding: 'utf8' }); } catch {}
    attachHandlers(child, attemptedExe, tried);
  }

  function prepareSpawnArgs() {
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
      spawnArgs.push(
        ".",
        "--exclude",
        "node_modules|.git|.vite|dist|build|.cache"
      );
    if (!options.execPath) {
      exe = "npx";
      spawnArgs.unshift("rds");
    }
    return spawnArgs;
  }

  async function start() {
    if (serverClosed) return;
    if (child) return;

    try {
      await stopChild();
    } catch {}

    if (serverClosed) return;
    if (child) return;

    // Ensure any orphaned pid recorded by a previous plugin instance is killed
    // before we spawn a new process. Wait a short while (up to 5s) for it to
    // disappear to avoid races where a lingering process and a new one both run.
    await killPidFromFile().catch(() => {});
    function sleep(ms) { return new Promise((r) => setTimeout(r, ms)); }
    function isPidAlive(pid) {
      try {
        if (!pid) return false;
        if (process.platform === 'win32') {
          const r = spawnSync('tasklist', ['/FI', `PID eq ${pid}`], { encoding: 'utf8' });
          return !!(r && r.stdout && r.stdout.indexOf(String(pid)) !== -1);
        }
        process.kill(pid, 0);
        return true;
      } catch {
        return false;
      }
    }
    try {
      if (fs.existsSync(pidFile)) {
        const txt = fs.readFileSync(pidFile, 'utf8').trim();
        const pid = Number(txt) || null;
        if (pid) {
          let waited = 0;
          const maxWait = 5000;
          while (isPidAlive(pid) && waited < maxWait) {
            // eslint-disable-next-line no-await-in-loop
            await sleep(200);
            waited += 200;
          }
          if (isPidAlive(pid)) {
            // last attempt
            await killPidFromFile().catch(() => {});
            // If still alive, try killing by image name as a last resort
            if (isPidAlive(pid)) {
              await killByImageName().catch(() => {});
            }
            // give it a short moment
            // eslint-disable-next-line no-await-in-loop
            await sleep(200);
          }
        }
      }
    } catch {}

    // As an extra safeguard: kill by image name and wait for any rds
    // processes to disappear before spawning a new one.
    await killByImageName().catch(() => {});
    function anyRdsRunning() {
      try {
        if (process.platform === 'win32') {
          const r = spawnSync('tasklist', ['/FI', 'IMAGENAME eq rds.exe'], { encoding: 'utf8' });
          return !!(r && r.stdout && r.stdout.indexOf('rds.exe') !== -1);
        }
        const r = spawnSync('pgrep', ['-f', 'rds'], { encoding: 'utf8' });
        return !!(r && r.status === 0 && r.stdout && r.stdout.trim());
      } catch {
        return false;
      }
    }
    const waitStart = Date.now();
    while (anyRdsRunning() && Date.now() - waitStart < 3000) {
      // eslint-disable-next-line no-await-in-loop
      await new Promise((r) => setTimeout(r, 200));
    }

    const spawnArgs = prepareSpawnArgs();
    const spawnArgsForShell = spawnArgs.map(quoteIfNeeded);

    let resolved = resolveCommand(exe);
    const spawnOptions = { cwd, stdio: "inherit", shell: useShell };
    if (process.platform === "win32" && useShell) {
      const found = findExecutableOnPath(exe);
      if (found) {
        const real = followShimIfNeeded(found);
        if (real) resolved = real;
        spawnOptions.shell = false;
      }
    }
    const argsToPass = spawnOptions.shell ? spawnArgsForShell : spawnArgs;
  setChild(spawn(resolved, argsToPass, spawnOptions), "rds", { npx: false, npm: false });
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
          try {
            if (fs.existsSync(pidFile)) fs.unlinkSync(pidFile);
          } catch {}
          return resolve();
        }
      }, 200);
    });
  }

  // Try to kill a PID recorded in the pidFile. This helps when the plugin is
  // re-initialized and the previous instance left an orphaned rds process.
  async function killPidFromFile() {
    try {
      if (!fs.existsSync(pidFile)) return;
      const txt = fs.readFileSync(pidFile, "utf8").trim();
      const pid = Number(txt) || null;
      if (!pid) {
        try { fs.unlinkSync(pidFile); } catch {}
        return;
      }

      function isPidAliveLocal(p) {
        try {
          if (!p) return false;
          if (process.platform === 'win32') {
            const r = spawnSync('tasklist', ['/FI', `PID eq ${p}`], { encoding: 'utf8' });
            return !!(r && r.stdout && r.stdout.indexOf(String(p)) !== -1);
          }
          process.kill(p, 0);
          return true;
        } catch {
          return false;
        }
      }

      // Try to kill the pid synchronously on Windows, otherwise send SIGKILL.
      if (process.platform === 'win32') {
        try { spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore', shell: true }); } catch {}
      } else {
        try { process.kill(pid, 'SIGKILL'); } catch {}
      }

      // Wait up to 5 seconds for the pid to disappear.
      const start = Date.now();
      while (isPidAliveLocal(pid) && Date.now() - start < 5000) {
        // eslint-disable-next-line no-await-in-loop
        await new Promise((r) => setTimeout(r, 200));
      }

      try { if (fs.existsSync(pidFile)) fs.unlinkSync(pidFile); } catch {}
    } catch (err) {
      try { console.debug && console.debug('[vite-plugin-rds-watch] killPidFromFile failed', err && err.message); } catch {}
    }
  }

  async function killByImageName() {
    try {
      if (process.platform === 'win32') {
        try { spawnSync('taskkill', ['/F', '/IM', 'rds.exe', '/T'], { stdio: 'ignore', shell: true }); } catch {}
      } else {
        try { spawnSync('pkill', ['-f', 'rds'], { stdio: 'ignore' }); } catch {}
      }
    } catch (err) {
      try { console.debug && console.debug('[vite-plugin-rds-watch] killByImageName failed', err && err.message); } catch {}
    }
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
    async configureServer(server) {
      // Ensure any previously-running rds child is stopped before starting a new one.
      // Use stopChild() here so we don't set the plugin into a permanently-closed state.
      await stopChild().catch((err) => {
        // best-effort: if stopChild fails, surface debug info but continue startup
        try {
          console.debug &&
            console.debug("[vite-plugin-rds-watch] stopChild() failed", err);
        } catch {}
      });
      await killPidFromFile().catch(() => {});
      start();
      if (stopOnServerClose)
        server.httpServer && server.httpServer.on("close", () => stop());
    },
    async handleHotUpdate(ctx) {
      const file = ctx.file || "";
      if (
        restartOnHotUpdate &&
        (file.endsWith("rds.config.toml") || file.includes("rds"))
      ) {
        // For restarts we only need to stop the running child process, not mark
        // the plugin as permanently closed.
        await stopChild().catch(() => {});
        await killPidFromFile().catch(() => {});
        start();
      }
      return [];
    },
    closeBundle() {
      stop();
    },
  };
};
