#!/usr/bin/env node
/**
 * Pack Chrome extension into a production CRX3 file and Firefox XPI file.
 * Zero external dependencies.
 */
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execSync } = require('child_process');

function encodeVarint(n) {
  const res = [];
  while (true) {
    const b = n & 0x7F;
    n >>>= 7;
    if (n) {
      res.push(b | 0x80);
    } else {
      res.push(b);
      break;
    }
  }
  return Buffer.from(res);
}

function buildZip(sourceDir, zipPath) {
  if (process.platform === 'win32') {
    const psCmd = `Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::CreateFromDirectory('${sourceDir.replace(/'/g, "''")}', '${zipPath.replace(/'/g, "''")}')`;
    if (fs.existsSync(zipPath)) fs.unlinkSync(zipPath);
    execSync(`powershell -NoProfile -Command "${psCmd}"`, { stdio: 'pipe' });
  } else {
    execSync(`cd "${sourceDir}" && zip -qr "${zipPath}" .`, { stdio: 'pipe' });
  }
}

function packCrx(zipBytes, privateKeyPem) {
  const priv = crypto.createPrivateKey(privateKeyPem);
  const pub = crypto.createPublicKey(priv);
  const pubDer = pub.export({ type: 'spki', format: 'der' });

  const crxId = crypto.createHash('sha256').update(pubDer).digest().slice(0, 16);
  const signedHeaderData = Buffer.concat([Buffer.from([0x0A, 0x10]), crxId]);

  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32LE(signedHeaderData.length, 0);

  const toSign = Buffer.concat([
    Buffer.from('CRX3 SignedData\x00'),
    lenBuf,
    signedHeaderData,
    zipBytes,
  ]);

  const sig = crypto.sign('sha256', toSign, priv);

  const proof = Buffer.concat([
    Buffer.from([0x0A]),
    encodeVarint(pubDer.length),
    pubDer,
    Buffer.from([0x12]),
    encodeVarint(sig.length),
    sig,
  ]);

  const header = Buffer.concat([
    Buffer.from([0x12]),
    encodeVarint(proof.length),
    proof,
    Buffer.from([0x82, 0xF1, 0x04]),
    encodeVarint(signedHeaderData.length),
    signedHeaderData,
  ]);

  const headLenBuf = Buffer.alloc(4);
  headLenBuf.writeUInt32LE(header.length, 0);

  const verBuf = Buffer.alloc(4);
  verBuf.writeUInt32LE(3, 0);

  return Buffer.concat([
    Buffer.from('Cr24'),
    verBuf,
    headLenBuf,
    header,
    zipBytes,
  ]);
}

function copyDirClean(src, dest, isFirefox = false) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const s = path.join(src, entry.name);
    const d = path.join(dest, entry.name);
    if (entry.name.endsWith('.test.js') || entry.name.endsWith('.md') || entry.name === 'testdata' || entry.name.endsWith('.ps1') || entry.name.endsWith('.pem') || entry.name === 'key.pem') {
      continue;
    }
    if (isFirefox && (entry.name === 'offscreen.html' || entry.name === 'offscreen.js' || entry.name === 'manifest-firefox.json')) {
      continue;
    }
    if (entry.isDirectory()) {
      copyDirClean(s, d, isFirefox);
    } else {
      fs.copyFileSync(s, d);
    }
  }
}

function main() {
  const repo = path.resolve(__dirname, '..');
  const source = path.join(repo, 'browser-extension');
  const keyPath = path.join(source, 'key.pem');

  if (!fs.existsSync(keyPath)) {
    console.error(`Private key not found at ${keyPath}`);
    process.exit(1);
  }

  const pem = fs.readFileSync(keyPath);

  const outDir = process.argv[2] ? path.resolve(process.argv[2]) : repo;
  fs.mkdirSync(outDir, { recursive: true });

  // 1. Unpacked Chrome folder
  const chromeDir = path.join(outDir, 'browser-extension-chrome');
  copyDirClean(source, chromeDir, false);

  // Compat folder
  const compatDir = path.join(outDir, 'browser-extension');
  if (compatDir !== chromeDir) {
    copyDirClean(source, compatDir, false);
  }

  const tempChromeZip = path.join(outDir, `_temp_chrome_${Date.now()}.zip`);
  buildZip(chromeDir, tempChromeZip);
  const chromeZipBytes = fs.readFileSync(tempChromeZip);
  fs.unlinkSync(tempChromeZip);

  // 2. Unpacked Firefox folder
  const firefoxDir = path.join(outDir, 'browser-extension-firefox');
  copyDirClean(source, firefoxDir, true);
  if (fs.existsSync(path.join(source, 'manifest-firefox.json'))) {
    fs.copyFileSync(path.join(source, 'manifest-firefox.json'), path.join(firefoxDir, 'manifest.json'));
  }
  const tempFirefoxZip = path.join(outDir, `_temp_firefox_${Date.now()}.zip`);
  buildZip(firefoxDir, tempFirefoxZip);
  const firefoxZipBytes = fs.readFileSync(tempFirefoxZip);
  fs.unlinkSync(tempFirefoxZip);

  // 3. Single packaged files per browser
  const standardCrx = path.join(outDir, 'SA2Downloader-Chrome.crx');
  const crxBytes = packCrx(chromeZipBytes, pem);
  fs.writeFileSync(standardCrx, crxBytes);

  const standardXpi = path.join(outDir, 'SA2Downloader-Firefox.xpi');
  fs.writeFileSync(standardXpi, firefoxZipBytes);

  // 4. Quick install helper for Chrome
  const chromeBatPath = path.join(outDir, 'install_chrome_extension.bat');
  const chromeBatContent = `@echo off\r\ntitle SA2 Downloader - Chrome Extension Helper\r\necho ==========================================================\r\necho   SA2 Downloader - Chrome Extension Installer\r\necho ==========================================================\r\necho.\r\necho Configuring extension policy allowlist (Chrome, Edge, Brave)...\r\nreg add "HKCU\\Software\\Policies\\Google\\Chrome\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKCU\\Software\\Policies\\Google\\Chrome\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\Google\\Chrome\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\Google\\Chrome\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\nreg add "HKCU\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKCU\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\nreg add "HKCU\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKCU\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallAllowlist" /v 1 /t REG_SZ /d "lekpmkgkkhcfocbigmdjnoalfnidjlal" /f >nul 2>&1\r\nreg add "HKLM\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallSources" /v 1 /t REG_SZ /d "<all_urls>" /f >nul 2>&1\r\necho Done!\r\necho.\r\necho Opening Chrome Extensions page...\r\nstart chrome "chrome://extensions"\r\necho.\r\necho ==========================================================\r\necho HOW TO ACTIVATE IN CHROME:\r\necho ==========================================================\r\necho.\r\necho Option 1 (No Warnings - Recommended):\r\necho   1. On chrome://extensions, click "Load unpacked" (top-left).\r\necho   2. Select the "browser-extension-chrome" folder.\r\necho      (Or simply drag the "browser-extension-chrome" folder into Chrome!)\r\necho.\r\necho Option 2 (If using the .crx file):\r\necho   1. Simply click the toggle switch on the extension card to turn it ON!\r\necho   * Note: The yellow banner is Chrome's default prompt for off-store CRX files.\r\necho     Turning the switch ON activates the extension immediately!\r\necho ==========================================================\r\necho.\r\npause\r\n`;
  fs.writeFileSync(chromeBatPath, chromeBatContent);

  // 5. Quick install helper for Firefox
  const firefoxBatPath = path.join(outDir, 'install_firefox_extension.bat');
  const firefoxBatContent = `@echo off\r\ntitle SA2 Downloader - Firefox Extension Helper\r\necho ==========================================================\r\necho   SA2 Downloader - Firefox Extension Installer\r\necho ==========================================================\r\necho.\r\necho Opening Firefox Add-ons page...\r\nstart firefox "about:addons"\r\necho.\r\necho INSTALLATION STEPS:\r\necho   1. Simply drag and drop "SA2Downloader-Firefox.xpi" onto Firefox window!\r\necho   2. Click "Add" when Firefox prompts.\r\necho ==========================================================\r\necho.\r\npause\r\n`;
  fs.writeFileSync(firefoxBatPath, firefoxBatContent);

  console.log(`Generated Chrome Extension Folder: ${chromeDir}`);
  console.log(`Generated Firefox Extension Folder: ${firefoxDir}`);
  console.log(`Generated Chrome CRX: ${standardCrx} (${(crxBytes.length / 1024).toFixed(1)} KB)`);
  console.log(`Generated Firefox XPI: ${standardXpi} (${(firefoxZipBytes.length / 1024).toFixed(1)} KB)`);
}

if (require.main === module) {
  main();
}

module.exports = { packCrx, buildZip };
