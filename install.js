#!/usr/bin/env node

const fs = require("fs");
const path = require("path");
// For future GitHub releases download functionality
// const https = require('https');
// const { execSync } = require('child_process');
// const GITHUB_REPO = 'Khallister/rds';
// const VERSION = require('./package.json').version;

function getPlatformInfo() {
  const platform = process.platform;
  const arch = process.arch;

  const platformMap = {
    win32: "windows",
    darwin: "macos",
    linux: "linux",
  };

  const archMap = {
    x64: "x64",
    arm64: "arm64",
  };

  return {
    platform: platformMap[platform] || platform,
    arch: archMap[arch] || arch,
    ext: platform === "win32" ? ".exe" : "",
  };
}

function downloadBinary() {
  const { platform, arch, ext } = getPlatformInfo();
  const binDir = path.join(__dirname, "bin");
  const finalBinaryName = `rds${ext}`;
  const finalBinaryPath = path.join(binDir, finalBinaryName);

  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }

  // 1) If there is a platform-specific prebuilt in bin/, prefer that.
  const candidateNames = [
    // exact platform-arch
    `rds-${platform}-${arch}${ext}`,
    `rds-${platform}${ext}`,
    // generic name (fallback)
    `rds${ext}`,
  ];

  for (const name of candidateNames) {
    const candidate = path.join(binDir, name);
    if (fs.existsSync(candidate)) {
      console.log(`📦 Using bundled binary: ${name}`);
      fs.copyFileSync(candidate, finalBinaryPath);
      if (process.platform !== "win32") {
        fs.chmodSync(finalBinaryPath, 0o755);
      }
      console.log("✅ RDS binary installed successfully!");
      return;
    }
  }

  // 2) If a local build exists in target/release, use that.
  const localBinary = path.join(__dirname, "target", "release", `rds${ext}`);
  if (fs.existsSync(localBinary)) {
    console.log("📦 Using local build binary...");
    fs.copyFileSync(localBinary, finalBinaryPath);
    if (process.platform !== "win32") {
      fs.chmodSync(finalBinaryPath, 0o755);
    }
    console.log("✅ RDS binary installed successfully!");
    return;
  }

  // TODO: In a full implementation, download from GitHub releases
  console.log(`📥 No matching binary found for ${platform}-${arch}`);
  console.log(
    `📥 Would download binary for ${platform}-${arch} from GitHub releases`,
  );
  console.log("⚠️  For now, please build locally with: cargo build --release");
  console.log("    Then copy target/release/rds to bin/rds (or bin/rds.exe on Windows)");
}

function main() {
  console.log("🚀 Installing RDS binary...");

  try {
    downloadBinary();
  } catch (error) {
    console.error("❌ Installation failed:", error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}
