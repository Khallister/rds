#!/usr/bin/env node

//
const fs = require("fs");
const path = require("path");
const https = require("https");
const { spawnSync } = require("child_process");
const VERSION = require("./package.json").version;
const argv = require('minimist')(process.argv.slice(2));
const CLI_TOKEN = argv.token || argv.t || null;
const DEBUG = argv.debug || argv.d || false;

function getPlatformInfo() {
  const p = process.platform;
  const a = process.arch;
  const pm = { win32: "windows", darwin: "macos", linux: "linux" };
  const am = { x64: "x64", arm64: "arm64" };
  return {
    platform: pm[p] || p,
    arch: am[a] || a,
    ext: p === "win32" ? ".exe" : "",
  };
}

function ghGetJson(pathname) {
  const headers = { "user-agent": "rds-installer" };
  if (CLI_TOKEN) headers.authorization = `Bearer ${CLI_TOKEN}`;
  else if (process.env.GITHUB_TOKEN)
    headers.authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const opts = {
    hostname: "api.github.com",
    path: pathname,
    headers,
    method: "GET",
  };
  return new Promise((resolve, reject) => {
    const req = https.request(opts, (res) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (d) => (body += d));
      res.on("end", () => {
        try {
          if (res.statusCode >= 400) {
                if (DEBUG) {
                  console.error(
                    `[rds-installer] GitHub API ${pathname} returned HTTP ${res.statusCode}`
                  );
                  console.error("[rds-installer] body:", body);
                }
                return resolve(null);
              }
          resolve(JSON.parse(body));
        } catch (err) {
          if (DEBUG)
            console.error(
              "[rds-installer] JSON parse error for",
              pathname,
              err
            );
          reject(err);
        }
      });
    });
    req.on("error", reject);
    req.end();
  });
}

async function fetchReleaseInfo(version) {
  const candidates = [];
  if (version && String(version).startsWith("v")) candidates.push(version);
  else {
    candidates.push(version);
    candidates.push(`v${version}`);
  }
  for (const tag of candidates) {
    if (DEBUG)
      console.log(`[rds-installer] Looking up release by tag: ${tag}`);
    const byTag = await ghGetJson(`/repos/Khallister/rds/releases/tags/${tag}`);
    if (byTag && byTag.tag_name) return byTag;
  }
  if (DEBUG) console.log('[rds-installer] Tag lookups exhausted');
  if (DEBUG)
      console.log("[rds-installer] Tag lookups failed, trying latest");
  const latest = await ghGetJson("/repos/Khallister/rds/releases/latest");
  if (DEBUG && latest) {
    try {
      console.log('[rds-installer] Latest release found:', latest.tag_name);
      if (Array.isArray(latest.assets)) console.log('[rds-installer] latest.assets:', latest.assets.map(a=>a.name));
    } catch (e) {
      if (DEBUG) console.error('[rds-installer] error logging latest release', e);
    }
  }
  if (!latest && DEBUG)
    console.log("[rds-installer] Latest release lookup failed");
  return latest && latest.tag_name ? latest : null;
}

function findAssetForRelease(release, candidates) {
  if (!release || !Array.isArray(release.assets)) return null;
  if (DEBUG) console.log('[rds-installer] release.assets:', release.assets.map(a=>a.name));
  for (const name of candidates) {
    if (DEBUG) console.log(`[rds-installer] looking for asset name: ${name}`);
    const a = release.assets.find((x) => x.name === name);
    if (a) return a;
  }
  if (DEBUG) console.log('[rds-installer] no matching assets found for candidates:', candidates);
  return null;
}

function downloadToFile(url, dest, headers = {}) {
  return new Promise((resolve, reject) => {
    const maxRedirects = 5;
    const doGet = function doGet(urlToGet, hdrs, redirectsLeft) {
      if (DEBUG) console.log(`[rds-installer] downloadToFile: requesting ${urlToGet} (redirectsLeft=${redirectsLeft})`);
      if (redirectsLeft < 0) return reject(new Error('Too many redirects'));
      const out = fs.createWriteStream(dest);
      const opts = new URL(urlToGet);
      opts.headers = { "user-agent": "rds-installer", ...hdrs };
      const req = https.get(opts, (res) => {
        if (DEBUG) console.log(`[rds-installer] downloadToFile: got HTTP ${res.statusCode} for ${urlToGet}`);
        // follow redirects
        if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers && res.headers.location) {
          try { out.close(); fs.unlinkSync(dest); } catch (err) { if (DEBUG) console.error('cleanup failed', err); }
          const loc = res.headers.location;
          // if redirecting off GitHub, drop authorization header
          let nextHeaders = { ...hdrs };
          try {
            const locUrl = new URL(loc);
            if (locUrl.hostname !== opts.hostname) delete nextHeaders.authorization;
          } catch (err) { if (DEBUG) console.error('bad redirect URL', err); }
          res.resume();
          return doGet(loc, nextHeaders, redirectsLeft - 1);
        }
          if (res.statusCode && res.statusCode >= 400) {
          try { out.close(); fs.unlinkSync(dest); } catch (err) { if (DEBUG) console.error('cleanup failed', err); }
          return reject(new Error(`HTTP ${res.statusCode}`));
        }
        res.pipe(out);
  out.on("finish", () => out.close(resolve));
      });
      req.on("error", (err) => {
        try { fs.unlinkSync(dest); } catch (err2) { if (DEBUG) console.error('cleanup failed', err2); }
        reject(err);
      });
    };
    doGet(url, headers, maxRedirects);
  });
}

function programExists(cmd) {
  try {
    const which = process.platform === "win32" ? "where" : "which";
    const r = spawnSync(which, [cmd], { stdio: "ignore" });
    return r.status === 0;
  } catch (e) {
    if (DEBUG) console.error(e);
    return false;
  }
}

function extractArchive(archivePath, destDir) {
  // prefer zip handling on Windows via PowerShell
  if (archivePath.endsWith('.zip')) {
    if (process.platform === 'win32') {
      const r = spawnSync('powershell', ['-Command', `Expand-Archive -Force -Path '${archivePath}' -DestinationPath '${destDir}'`], { stdio: 'inherit' });
      return r.status === 0;
    }
    if (programExists("unzip")) {
      const r = spawnSync("unzip", ["-o", archivePath, "-d", destDir], {
        stdio: "inherit",
      });
      return r.status === 0;
    }
  }
  // handle tar.gz
  if (archivePath.endsWith('.tar.gz')) {
    if (programExists("tar")) {
      const r = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], {
        stdio: "inherit",
      });
      return r.status === 0;
    }
  }
  return false;
}

function _authHeaders() {
  const headers = { "user-agent": "rds-installer" };
  const token = CLI_TOKEN || process.env.GITHUB_TOKEN;
  if (token) headers.authorization = `Bearer ${token}`;
  return headers;
}

async function downloadAndPlaceFile(url, dest) {
  // Use the API asset URL with an explicit Accept header to get the raw binary
  await downloadToFile(url, dest, { ..._authHeaders(), Accept: 'application/octet-stream' });
  if (process.platform !== "win32") fs.chmodSync(dest, 0o755);
}

async function downloadAndExtractArchive(asset, binDir, ext) {
  const tmpName = path.join(binDir, asset.name);
  // Prefer the API asset URL and request the raw binary
  const assetUrl = asset.url || asset.browser_download_url;
  await downloadToFile(assetUrl, tmpName, { ..._authHeaders(), Accept: 'application/octet-stream' });
  const ok = extractArchive(tmpName, binDir);
  if (!ok) return false;
  const extractedPath = path.join(binDir, `rds${ext}`);
  if (!fs.existsSync(extractedPath)) return false;
  if (process.platform !== "win32") fs.chmodSync(extractedPath, 0o755);
  try {
    fs.unlinkSync(tmpName);
  } catch (e) {
    if (DEBUG) console.error(e);
  }
  return true;
}

async function installAssetFromRelease(asset, isArchive, binDir, finalBinaryPath, ext) {
  if (!isArchive) {
    await downloadAndPlaceFile(asset.browser_download_url, finalBinaryPath);
    return true;
  }
  return await downloadAndExtractArchive(asset, binDir, ext);
}

async function ensureBinDir(binDir) {
  if (!fs.existsSync(binDir)) fs.mkdirSync(binDir, { recursive: true });
}

function tryBundled(binDir, finalBinaryPath, candidateNames) {
  for (const name of candidateNames) {
    const candidate = path.join(binDir, name);
    if (fs.existsSync(candidate)) {
      fs.copyFileSync(candidate, finalBinaryPath);
      if (process.platform !== "win32") fs.chmodSync(finalBinaryPath, 0o755);
      console.log(`Using bundled binary: ${name}`);
      return true;
    }
  }
  return false;
}

function tryLocalBuild(finalBinaryPath, ext) {
  const localBinary = path.join(__dirname, "target", "release", `rds${ext}`);
  if (fs.existsSync(localBinary)) {
    fs.copyFileSync(localBinary, finalBinaryPath);
    if (process.platform !== "win32") fs.chmodSync(finalBinaryPath, 0o755);
    console.log("Using local build binary");
    return true;
  }
  return false;
}

async function tryInstallFromRelease(release, platform, arch, ext, binDir, finalBinaryPath) {
  // Support alternate arch naming used by CI artifacts (e.g. x64 -> x86_64)
  const altArchMap = { x64: "x86_64", arm64: "arm64" };
  const altArch = altArchMap[arch] || arch;

  const fileCandidates = [
    `rds-${platform}-${arch}${ext}`,
    `rds-${platform}-${altArch}${ext}`,
    `rds-${platform}${ext}`,
    `rds${ext}`,
  ];

  const archiveCandidates = [
    `rds-${platform}-${arch}.zip`,
    `rds-${platform}-${altArch}.zip`,
    `rds-${platform}-${arch}.tar.gz`,
    `rds-${platform}-${altArch}.tar.gz`,
    `rds-${platform}.zip`,
    `rds-${platform}.tar.gz`,
  ];

  if (DEBUG) console.log('[rds-installer] candidate file names:', fileCandidates, archiveCandidates);

  const fileAsset = findAssetForRelease(release, fileCandidates);
  const archiveAsset = findAssetForRelease(release, archiveCandidates);

  let foundAsset = null;
  let assetIsArchive;
  if (fileAsset) {
    foundAsset = fileAsset;
    assetIsArchive = false;
  } else if (archiveAsset) {
    foundAsset = archiveAsset;
    assetIsArchive = true;
  } else {
    return false;
  }

  return await installAssetFromRelease(foundAsset, assetIsArchive, binDir, finalBinaryPath, ext);
}

async function downloadBinary() {
  const { platform, arch, ext } = getPlatformInfo();
  const binDir = path.join(__dirname, "bin");
  const finalBinaryName = `rds${ext}`;
  const finalBinaryPath = path.join(binDir, finalBinaryName);
  await ensureBinDir(binDir);
  const candidateNames = [
    `rds-${platform}-${arch}${ext}`,
    `rds-${platform}${ext}`,
    `rds${ext}`,
  ];
  if (tryBundled(binDir, finalBinaryPath, candidateNames)) return;
  if (tryLocalBuild(finalBinaryPath, ext)) return;
  const release = await fetchReleaseInfo(VERSION);
  if (!release) {
    console.log("Could not fetch release info from GitHub");
    console.log("Please build locally with: cargo build --release and copy the binary to bin/");
    return;
  }
  try {
    const ok = await tryInstallFromRelease(release, platform, arch, ext, binDir, finalBinaryPath);
    if (ok) {
      console.log("RDS binary installed from release");
      return;
    }
  } catch (err) {
    console.error("Failed to download or install release asset:", err && err.message ? err.message : err);
    console.log("Please build locally with: cargo build --release and copy the binary to bin/");
    return;
  }
  console.log("Could not install binary automatically; please install manually");
}

function main() {
  Promise.resolve(downloadBinary()).catch((error) => {
    console.error(
      "Installation failed:",
      error && error.message ? error.message : error
    );
    process.exit(1);
  });
}

if (require.main === module) main();
