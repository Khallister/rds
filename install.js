#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const https = require("https");
const { spawnSync } = require("child_process");
const VERSION = require("./package.json").version;
const argv = require('minimist')(process.argv.slice(2));
const CLI_TOKEN = argv.token || argv.t || null;

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
  if (CLI_TOKEN) headers.authorization = `token ${CLI_TOKEN}`;
  else if (process.env.GITHUB_TOKEN)
    headers.authorization = `token ${process.env.GITHUB_TOKEN}`;
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
                if (process.env.RDS_DEBUG) {
                  console.error(
                    `[rds-installer] GitHub API ${pathname} returned HTTP ${res.statusCode}`
                  );
                  console.error("[rds-installer] body:", body);
                }
                return resolve(null);
              }
          resolve(JSON.parse(body));
        } catch (err) {
          if (process.env.RDS_DEBUG)
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
    if (process.env.RDS_DEBUG)
      console.log(`[rds-installer] Looking up release by tag: ${tag}`);
    const byTag = await ghGetJson(`/repos/Khallister/rds/releases/tags/${tag}`);
    if (byTag && byTag.tag_name) return byTag;
  }
  if (process.env.RDS_DEBUG)
    console.log("[rds-installer] Tag lookups failed, trying latest");
  const latest = await ghGetJson("/repos/Khallister/rds/releases/latest");
  if (!latest && process.env.RDS_DEBUG)
    console.log("[rds-installer] Latest release lookup failed");
  return latest && latest.tag_name ? latest : null;
}

function findAssetForRelease(release, candidates) {
  if (!release || !Array.isArray(release.assets)) return null;
  for (const name of candidates) {
    const a = release.assets.find((x) => x.name === name);
    if (a) return a;
  }
  return null;
}

function downloadToFile(url, dest, headers = {}) {
  return new Promise((resolve, reject) => {
    const out = fs.createWriteStream(dest);
    const opts = new URL(url);
    opts.headers = { "user-agent": "rds-installer", ...headers };
    https
      .get(opts, (res) => {
        if (res.statusCode && res.statusCode >= 400)
          return reject(new Error(`HTTP ${res.statusCode}`));
        res.pipe(out);
        out.on("finish", () => out.close(resolve));
      })
      .on("error", (err) => {
        try {
          fs.unlinkSync(dest);
        } catch (e) {
          if (process.env.RDS_DEBUG) console.error(e);
        }
        reject(err);
      });
  });
}

function programExists(cmd) {
  try {
    const which = process.platform === "win32" ? "where" : "which";
    const r = spawnSync(which, [cmd], { stdio: "ignore" });
    return r.status === 0;
  } catch (e) {
    if (process.env.RDS_DEBUG) console.error(e);
    return false;
  }
}

function extractArchive(archivePath, destDir) {
  if (programExists("unzip")) {
    const r = spawnSync("unzip", ["-o", archivePath, "-d", destDir], {
      stdio: "inherit",
    });
    return r.status === 0;
  }
  if (programExists("tar")) {
    const r = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], {
      stdio: "inherit",
    });
    return r.status === 0;
  }
  return false;
}

function _authHeaders() {
  const headers = { "user-agent": "rds-installer" };
  const token = CLI_TOKEN || process.env.GITHUB_TOKEN;
  if (token) headers.authorization = `token ${token}`;
  return headers;
}

async function downloadAndPlaceFile(url, dest) {
  await downloadToFile(url, dest, _authHeaders());
  if (process.platform !== "win32") fs.chmodSync(dest, 0o755);
}

async function downloadAndExtractArchive(asset, binDir, ext) {
  const tmpName = path.join(binDir, asset.name);
  await downloadToFile(asset.browser_download_url, tmpName, _authHeaders());
  const ok = extractArchive(tmpName, binDir);
  if (!ok) return false;
  const extractedPath = path.join(binDir, `rds${ext}`);
  if (!fs.existsSync(extractedPath)) return false;
  if (process.platform !== "win32") fs.chmodSync(extractedPath, 0o755);
  try {
    fs.unlinkSync(tmpName);
  } catch (e) {
    if (process.env.RDS_DEBUG) console.error(e);
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
  const candidates = [
    `rds-${platform}-${arch}${ext}`,
    `rds-${platform}${ext}`,
    `rds${ext}`,
  ];
  const archiveCandidates = [
    `rds-${platform}-${arch}.zip`,
    `rds-${platform}-${arch}.tar.gz`,
    `rds-${platform}.zip`,
    `rds-${platform}.tar.gz`,
  ];
  const foundAsset = findAssetForRelease(release, candidates) || findAssetForRelease(release, archiveCandidates);
  const assetIsArchive = !!findAssetForRelease(release, archiveCandidates);
  if (!foundAsset) return false;
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
