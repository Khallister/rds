#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
// For future GitHub releases download functionality
// const https = require('https');
// const { execSync } = require('child_process');
// const GITHUB_REPO = 'Khallister/rds';
// const VERSION = require('./package.json').version;

function getPlatformInfo() {
  const platform = process.platform;
  const arch = process.arch;
  
  // Map Node.js platform/arch to release naming convention
  const platformMap = {
    'win32': 'windows',
    'darwin': 'macos', 
    'linux': 'linux'
  };
  
  const archMap = {
    'x64': 'x64',
    'arm64': 'arm64'
  };
  
  return {
    platform: platformMap[platform] || platform,
    arch: archMap[arch] || arch,
    ext: platform === 'win32' ? '.exe' : ''
  };
}

function downloadBinary() {
  const { platform, arch, ext } = getPlatformInfo();
  const binaryName = `rds${ext}`;
  const binDir = path.join(__dirname, 'bin');
  const binaryPath = path.join(binDir, binaryName);
  
  // Create bin directory if it doesn't exist
  if (!fs.existsSync(binDir)) {
    fs.mkdirSync(binDir, { recursive: true });
  }
  
  // For now, copy from local build if available
  const localBinary = path.join(__dirname, 'target', 'release', binaryName);
  if (fs.existsSync(localBinary)) {
    console.log('📦 Using local binary...');
    fs.copyFileSync(localBinary, binaryPath);
    
    // Make executable on Unix systems
    if (process.platform !== 'win32') {
      fs.chmodSync(binaryPath, 0o755);
    }
    
    console.log('✅ RDS binary installed successfully!');
    return;
  }
  
  // TODO: In a full implementation, download from GitHub releases
  console.log(`📥 Would download binary for ${platform}-${arch} from GitHub releases`);
  console.log('⚠️  For now, please build locally with: cargo build --release');
  console.log('    Then copy target/release/rds to bin/rds');
}

function main() {
  console.log('🚀 Installing RDS binary...');
  
  try {
    downloadBinary();
  } catch (error) {
    console.error('❌ Installation failed:', error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}
