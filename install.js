#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
const https = require("https");
const os = require('os');
const { spawnSync } = require("child_process");
const VERSION = require("./package.json").version;
let argv;
try {
  argv = require('minimist')(process.argv.slice(2));
} catch {
  // Fallback minimal parser for install-time convenience when dev deps
  // (like minimist) are not installed. Supports --token/-t and --debug/-d
  argv = {};
  const raw = process.argv.slice(2);
  for (let i = 0; i < raw.length; i++) {
    const a = raw[i];
    if (a === '--token' || a === '-t') {
      argv.token = raw[i + 1];
      i++;
    } else if (a.startsWith('--token=')) {
      argv.token = a.split('=')[1];
    } else if (a === '--debug' || a === '-d') {
      argv.debug = true;
    } else if (a.startsWith('--debug=')) {
      argv.debug = a.split('=')[1] === 'true';
    } else if (!a.startsWith('-')) {
      // ignore positional args for this script
    }
  }
}
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

// Build a small list of tag candidates for a version string.
function buildTagCandidates(version) {
  if (version && String(version).startsWith('v')) return [version];
  return [version, `v${version}`];
}

// Try fetching release metadata by tag names in order, return first hit or null.
async function tryFetchByTags(tags) {
  for (const tag of tags) {
    if (DEBUG) console.log(`[rds-installer] Looking up release by tag: ${tag}`);
    const byTag = await ghGetJson(`/repos/Khallister/rds/releases/tags/${tag}`);
    if (byTag && byTag.tag_name) return byTag;
  }
  return null;
}

async function fetchReleaseInfo(version) {
  const candidates = buildTagCandidates(version);
  const byTag = await tryFetchByTags(candidates);
  if (byTag) return byTag;

  if (DEBUG) console.log('[rds-installer] Tag lookups exhausted');
  if (DEBUG) console.log('[rds-installer] Tag lookups failed, trying latest');

  const latest = await ghGetJson('/repos/Khallister/rds/releases/latest');
  if (DEBUG && latest) {
    // lightweight logging without try/catch to keep complexity low
    console.log('[rds-installer] Latest release found:', latest.tag_name);
    if (Array.isArray(latest.assets)) console.log('[rds-installer] latest.assets:', latest.assets.map(a => a.name));
  }
  if (!latest && DEBUG) console.log('[rds-installer] Latest release lookup failed');
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

// Helper: perform an HTTP GET following redirects up to redirectsLeft.
function performGet(urlToGet, hdrs, redirectsLeft) {
  return new Promise((resolve, reject) => {
    if (DEBUG) console.log(`[rds-installer] performGet: requesting ${urlToGet} (redirectsLeft=${redirectsLeft})`);
    if (redirectsLeft < 0) return reject(new Error('Too many redirects'));
    const opts = new URL(urlToGet);
    opts.headers = { "user-agent": "rds-installer", ...hdrs };
    const req = https.get(opts, (res) => {
      if (DEBUG) console.log(`[rds-installer] performGet: got HTTP ${res.statusCode} for ${urlToGet}`);
      // handle redirects
      if (res.statusCode && res.statusCode >= 300 && res.statusCode < 400 && res.headers && res.headers.location) {
        const loc = res.headers.location;
        // if redirecting off GitHub, drop authorization header
        let nextHeaders = { ...hdrs };
        try {
          const locUrl = new URL(loc);
          if (locUrl.hostname !== opts.hostname) delete nextHeaders.authorization;
        } catch (err) { if (DEBUG) console.error('bad redirect URL', err); }
        // consume and discard this response before following redirect
        res.resume();
        return resolve(performGet(loc, nextHeaders, redirectsLeft - 1));
      }
      if (res.statusCode && res.statusCode >= 400) {
        // consume and discard
        res.resume();
        return reject(new Error(`HTTP ${res.statusCode}`));
      }
      // success: return the response stream
      return resolve(res);
    });
    req.on('error', reject);
  });
}

// Helper: stream an HTTP response into a file path and resolve when finished.
function streamResponseToFile(res, dest) {
  return new Promise((resolve, reject) => {
    const out = fs.createWriteStream(dest);
    res.pipe(out);
    const cleanupOnError = (err) => {
      try { out.close(); fs.unlinkSync(dest); } catch (e) { if (DEBUG) console.error('cleanup failed', e); }
      reject(err);
    };
    out.on('finish', () => out.close(resolve));
    out.on('error', cleanupOnError);
    res.on('error', cleanupOnError);
  });
}

function downloadToFile(url, dest, headers = {}) {
  const maxRedirects = 5;
  return performGet(url, headers, maxRedirects).then((res) => streamResponseToFile(res, dest));
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

function runPowerShellExpandArchive(archivePath, destDir) {
  try {
    const psCmd = `Import-Module Microsoft.PowerShell.Archive -ErrorAction Stop; Expand-Archive -Force -Path '${archivePath}' -DestinationPath '${destDir}'`;
    const r = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', psCmd], { stdio: 'inherit' });
    return r.status === 0;
  } catch (e) {
    if (DEBUG) console.error('[rds-installer] powershell Expand-Archive attempt failed:', e && e.message ? e.message : e);
    return false;
  }
}

function run7zExtract(archivePath, destDir) {
  if (!programExists('7z')) return false;
  const r = spawnSync('7z', ['x', archivePath, `-o${destDir}`, '-y'], { stdio: 'inherit' });
  return r.status === 0;
}

function runUnzipExtract(archivePath, destDir) {
  if (!programExists('unzip')) return false;
  const r = spawnSync('unzip', ['-o', archivePath, '-d', destDir], { stdio: 'inherit' });
  return r.status === 0;
}

function runDotNetZipExtract(archivePath, destDir) {
  try {
    const escapedZip = archivePath.replace(/'/g, "''");
    const escapedDest = destDir.replace(/'/g, "''");
    const dotNetCmd = `Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::ExtractToDirectory('${escapedZip}','${escapedDest}')`;
    const r = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', dotNetCmd], { stdio: 'inherit' });
    return r.status === 0;
  } catch (e) {
    if (DEBUG) console.error('[rds-installer] dotnet-based zip extract attempt failed:', e && e.message ? e.message : e);
    return false;
  }
}

function tryZipWindows(archivePath, destDir) {
  if (runPowerShellExpandArchive(archivePath, destDir)) return true;
  if (run7zExtract(archivePath, destDir)) return true;
  if (runUnzipExtract(archivePath, destDir)) return true;
  if (runDotNetZipExtract(archivePath, destDir)) return true;
  if (DEBUG) console.error('[rds-installer] No available unzip method succeeded on Windows (Expand-Archive, 7z, unzip, or .NET fallback).');
  console.error('Note: PowerShell module import or script execution may be blocked by your execution policy.');
  console.error('Consider running `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` in an elevated PowerShell, or install 7-Zip and ensure it is on PATH.');
  return false;
}

function tryZipUnix(archivePath, destDir) {
  if (programExists('unzip')) {
    const r = spawnSync('unzip', ['-o', archivePath, '-d', destDir], { stdio: 'inherit' });
    if (r.status === 0) return true;
  }
  if (programExists('7z')) {
    const r = spawnSync('7z', ['x', archivePath, `-o${destDir}`, '-y'], { stdio: 'inherit' });
    if (r.status === 0) return true;
  }
  return false;
}

function tryTar(archivePath, destDir) {
  if (programExists('tar')) {
    const r = spawnSync('tar', ['-xzf', archivePath, '-C', destDir], { stdio: 'inherit' });
    if (r.status === 0) return true;
  }
  if (programExists('7z')) {
    const r2 = spawnSync('7z', ['x', archivePath, `-o${destDir}`, '-y'], { stdio: 'inherit' });
    if (r2.status === 0) return true;
  }
  return false;
}

function extractArchive(archivePath, destDir) {
  if (archivePath.endsWith('.zip')) {
    if (process.platform === 'win32') return tryZipWindows(archivePath, destDir);
    return tryZipUnix(archivePath, destDir);
  }
  if (archivePath.endsWith('.tar.gz') || archivePath.endsWith('.tgz')) {
    return tryTar(archivePath, destDir);
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
  // Download to a temp directory to avoid leaving archive files inside the package `bin/`.
  const tmpName = path.join(os.tmpdir(), `${Date.now()}-${asset.name}`);
  // Prefer the API asset URL and request the raw binary
  const assetUrl = asset.url || asset.browser_download_url;
  try {
    await downloadToFile(assetUrl, tmpName, { ..._authHeaders(), Accept: 'application/octet-stream' });
    const ok = extractArchive(tmpName, binDir);
    if (!ok) return false;
    const extractedPath = path.join(binDir, `rds${ext}`);
    if (!fs.existsSync(extractedPath)) return false;
    if (process.platform !== "win32") fs.chmodSync(extractedPath, 0o755);
    return true;
  } finally {
    // ensure temp file is removed regardless of success or failure
    try { if (fs.existsSync(tmpName)) fs.unlinkSync(tmpName); } catch (e) { if (DEBUG) console.error('[rds-installer] cleanup failed', e); }
  }
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

