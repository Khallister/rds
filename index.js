#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

// Determine the correct binary based on platform and architecture
function getBinaryPath() {
  const platform = process.platform;
  const arch = process.arch;
  
  let binaryName = 'rds';
  if (platform === 'win32') {
    binaryName += '.exe';
  }
  
      const binaryPath = path.join(__dirname, 'bin', binaryName);
  
  if (!fs.existsSync(binaryPath)) {
    console.error(`Binary not found for ${platform}-${arch}: ${binaryPath}`);
    console.error('Please ensure the correct binary is installed.');
    process.exit(1);
  }
  
  return binaryPath;
}

function main() {
  const binaryPath = getBinaryPath();
  const args = process.argv.slice(2);
  
  const child = spawn(binaryPath, args, {
    stdio: 'inherit',
    shell: false
  });
  
  child.on('exit', (code) => {
    process.exit(code);
  });
  
  child.on('error', (err) => {
    console.error('Failed to start RDS:', err.message);
    process.exit(1);
  });
}

if (require.main === module) {
  main();
}

module.exports = { getBinaryPath };
