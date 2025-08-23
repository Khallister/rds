#!/usr/bin/env node
const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const argv = require('minimist')(process.argv.slice(2), { boolean: ['dry-run', 'force'] });
const target = argv.target || null;
const customName = argv['binary-name'] || argv.name || null;
const dryRun = !!argv['dry-run'];
const force = !!argv['force'];

// Known mapping from target triple to platform/arch/archive format
const TRIPLE_MAP = {
  'x86_64-unknown-linux-gnu': { platform: 'linux', arch: 'x64', ext: '', archive: 'tar' },
  'aarch64-unknown-linux-gnu': { platform: 'linux', arch: 'arm64', ext: '', archive: 'tar' },
  'x86_64-pc-windows-gnu': { platform: 'windows', arch: 'x64', ext: '.exe', archive: 'zip' },
  'x86_64-pc-windows-msvc': { platform: 'windows', arch: 'x64', ext: '.exe', archive: 'zip' },
  'x86_64-apple-darwin': { platform: 'macos', arch: 'x64', ext: '', archive: 'tar' },
  'aarch64-apple-darwin': { platform: 'macos', arch: 'arm64', ext: '', archive: 'tar' },
};

function getBinDir() {
  return path.join(__dirname, '..', 'bin');
}

function ensureBinDir(dir) {
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
}

function lookupTriple(triple) {
  if (triple && TRIPLE_MAP[triple]) return TRIPLE_MAP[triple];

  if (!triple) {
    const plat = process.platform;
    const arch = process.arch === 'x64' ? 'x64' : process.arch;
    if (plat === 'win32') return { platform: 'windows', arch, ext: '.exe', archive: 'zip' };
    if (plat === 'darwin') return { platform: 'macos', arch, ext: '', archive: 'tar' };
    return { platform: 'linux', arch, ext: '', archive: 'tar' };
  }

  const patterns = [
    {
      match: t => t.includes('windows'),
      result: { platform: 'windows', arch: triple.includes('aarch64') ? 'arm64' : 'x64', ext: '.exe', archive: 'zip' },
    },
    {
      match: t => t.includes('darwin') || t.includes('apple'),
      result: { platform: 'macos', arch: triple.includes('aarch64') ? 'arm64' : 'x64', ext: '', archive: 'tar' },
    },
    {
      match: t => t.includes('linux'),
      result: { platform: 'linux', arch: triple.includes('aarch64') ? 'arm64' : 'x64', ext: '', archive: 'tar' },
    },
  ];

  const matched = patterns.find(p => p.match(triple));
  return matched ? matched.result : { platform: 'unknown', arch: 'x64', ext: '', archive: 'tar' };
}

function runCargoBuild(target) {
  const cargoArgs = ['build', '--release'];
  if (target) cargoArgs.push('--target', target);
  console.log('> cargo', cargoArgs.join(' '));
  return spawnSync('cargo', cargoArgs, { stdio: 'inherit' });
}

function ensureRustupTarget(target) {
  // try to install target via rustup if available
  const rustupCheck = spawnSync('rustup', ['--version']);
  if (rustupCheck.status !== 0) return { ok: false, reason: 'rustup-not-found' };
  console.log(`Detected rustup. Attempting to add target ${target}...`);
  const add = spawnSync('rustup', ['target', 'add', target], { stdio: 'inherit' });
  return { ok: add.status === 0, status: add.status };
}

function copyAndMakeExecutable(src, dest, makeExecutable) {
  fs.copyFileSync(src, dest);
  if (makeExecutable) fs.chmodSync(dest, 0o755);
}

function createArchive(finalPath, info, destDir) {
  const archiveBase = `rds-${info.platform}-${info.arch}`;
  const archivePath = info.archive === 'zip' ? path.join(destDir, `${archiveBase}.zip`) : path.join(destDir, `${archiveBase}.tar.gz`);
  let archiver;
  try {
    archiver = require('archiver');
  } catch (err) {
    console.error('Missing dependency: "archiver" is required to create archives. Install dependencies or run `npm install --save-dev archiver`. Error:', err && err.message ? err.message : err);
    process.exit(10);
  }
  const output = fs.createWriteStream(archivePath);
  const archive = archiver(info.archive === 'zip' ? 'zip' : 'tar', info.archive === 'zip' ? {} : { gzip: true });

  return new Promise((resolve, reject) => {
    output.on('close', () => resolve(archivePath));
    archive.on('error', err => reject(err));
    archive.pipe(output);
    const insideName = `rds${info.ext}`;
    archive.file(finalPath, { name: insideName });
    archive.finalize();
  });
}

async function main() {
  const binDir = getBinDir();
  ensureBinDir(binDir);

  const info = lookupTriple(target);
  const finalName = customName || `rds-${info.platform}-${info.arch}${info.ext}`;
  const finalPath = path.join(binDir, finalName);
  // argument validation and dry-run handling
  function validateArgs() {
    if (customName && (customName === '' || customName.includes(path.sep))) {
      console.error('Invalid --binary-name value. Provide a basename without path separators.');
      process.exit(2);
    }
  }

  function handleDryRun() {
    console.log('Dry run mode: no commands will be executed. Computed values:');
    console.log('  target:', target || '(native)');
    console.log('  final binary path:', finalPath);
    console.log('  archive type:', info.archive);
    console.log('  cargo command:', `cargo build --release${target ? ' --target ' + target : ''}`);
    console.log('  overwrite allowed (--force):', force);
    process.exit(0);
  }

  validateArgs();
  if (dryRun) handleDryRun();

  // Build (try, optionally auto-install rustup target)
  let res = runCargoBuild(target);
  if (res.error) {
    console.error('Failed to run cargo:', res.error.message);
    process.exit(2);
  }

  if (res.status !== 0 && target) {
    const installed = ensureRustupTarget(target);
    if (!installed.ok) {
      console.error('Could not install target automatically:', installed.reason || installed.status);
      process.exit(installed.status || 3);
    }
    // retry build
    res = runCargoBuild(target);
    if (res.status !== 0) {
      console.error('cargo build failed with status', res.status);
      process.exit(res.status || 3);
    }
  } else if (res.status !== 0) {
    console.error('cargo build failed with status', res.status);
    process.exit(res.status || 3);
  }

  const exeName = `rds${info.ext}`;
  const artifactPath = target ? path.join(__dirname, '..', 'target', target, 'release', exeName) : path.join(__dirname, '..', 'target', 'release', exeName);
  if (!fs.existsSync(artifactPath)) {
    console.error('Built artifact not found at', artifactPath);
    process.exit(4);
  }

  // prevent accidental overwrite
  if (fs.existsSync(finalPath) && !force) {
    console.error(`Destination ${finalPath} already exists. Use --force to overwrite.`);
    process.exit(5);
  }

  copyAndMakeExecutable(artifactPath, finalPath, info.ext === '');

  try {
    const archivePath = await createArchive(finalPath, info, binDir);
    console.log(`✅ Archive created at ${archivePath}`);
    console.log(`✅ Built and copied ${artifactPath} -> ${finalPath}`);
  } catch (err) {
    console.error('Failed to create archive:', err.message || err);
    process.exit(1);
  }
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
