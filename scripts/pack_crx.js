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
    if (entry.name.endsWith('.test.js') || entry.name.endsWith('.md') || entry.name === 'testdata' || entry.name.endsWith('.ps1') || entry.name.endsWith('.pem')) {
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
  const manifest = JSON.parse(fs.readFileSync(path.join(source, 'manifest.json'), 'utf8'));
  const version = manifest.version || '1.2.1';

  const outDir = process.argv[2] ? path.resolve(process.argv[2]) : repo;
  fs.mkdirSync(outDir, { recursive: true });

  // Stage Chrome
  const tempChromeDir = path.join(outDir, `_temp_chrome_${Date.now()}`);
  copyDirClean(source, tempChromeDir, false);
  const tempChromeZip = path.join(outDir, `_temp_chrome_${Date.now()}.zip`);
  buildZip(tempChromeDir, tempChromeZip);
  const chromeZipBytes = fs.readFileSync(tempChromeZip);

  // Stage Firefox
  const tempFirefoxDir = path.join(outDir, `_temp_firefox_${Date.now()}`);
  copyDirClean(source, tempFirefoxDir, true);
  fs.copyFileSync(path.join(source, 'manifest-firefox.json'), path.join(tempFirefoxDir, 'manifest.json'));
  const tempFirefoxZip = path.join(outDir, `_temp_firefox_${Date.now()}.zip`);
  buildZip(tempFirefoxDir, tempFirefoxZip);
  const firefoxZipBytes = fs.readFileSync(tempFirefoxZip);

  // Clean temp dirs
  fs.rmSync(tempChromeDir, { recursive: true, force: true });
  fs.rmSync(tempFirefoxDir, { recursive: true, force: true });
  fs.unlinkSync(tempChromeZip);
  fs.unlinkSync(tempFirefoxZip);

  // 1. Pack CRX3
  const crxBytes = packCrx(chromeZipBytes, pem);
  const versionedCrx = path.join(outDir, `SA2Downloader-Chrome-${version}.crx`);
  const standardCrx = path.join(outDir, `SA2Downloader-Chrome.crx`);
  const legacyCrx = path.join(outDir, `sa2_downloader-chrome.crx`);
  fs.writeFileSync(versionedCrx, crxBytes);
  fs.writeFileSync(standardCrx, crxBytes);
  fs.writeFileSync(legacyCrx, crxBytes);

  // 2. Pack Firefox XPI
  const versionedXpi = path.join(outDir, `SA2Downloader-Firefox-${version}.xpi`);
  const standardXpi = path.join(outDir, `SA2Downloader-Firefox.xpi`);
  const legacyXpi = path.join(outDir, `sa2_downloader-firefox.xpi`);
  fs.writeFileSync(versionedXpi, firefoxZipBytes);
  fs.writeFileSync(standardXpi, firefoxZipBytes);
  fs.writeFileSync(legacyXpi, firefoxZipBytes);

  // 3. Pack Chrome ZIP
  const versionedZip = path.join(outDir, `SA2Downloader-Chrome-${version}.zip`);
  const standardZip = path.join(outDir, `SA2Downloader-Chrome.zip`);
  fs.writeFileSync(versionedZip, chromeZipBytes);
  fs.writeFileSync(standardZip, chromeZipBytes);

  // 4. Quick install helper for Chrome
  const batPath = path.join(outDir, 'install_chrome_extension.bat');
  const batContent = `@echo off\r\ntitle SA2 Downloader - Chrome Extension Quick Installer\r\necho ===================================================\r\necho   SA2 Downloader - Chrome Extension Quick Installer\r\necho ===================================================\r\necho.\r\necho 1. Opening Chrome Extensions page...\r\nstart chrome "chrome://extensions"\r\necho.\r\necho 2. Turn ON "Developer mode" toggle in top-right corner.\r\necho 3. Drag and drop "SA2Downloader-Chrome.crx" (or the browser-extension folder)\r\necho    directly into your Chrome extensions window.\r\necho.\r\npause\r\n`;
  fs.writeFileSync(batPath, batContent);

  console.log(`Generated Chrome CRX: ${versionedCrx} (${(crxBytes.length / 1024).toFixed(1)} KB)`);
  console.log(`Generated Firefox XPI: ${versionedXpi} (${(firefoxZipBytes.length / 1024).toFixed(1)} KB)`);
  console.log(`Generated Chrome ZIP: ${versionedZip} (${(chromeZipBytes.length / 1024).toFixed(1)} KB)`);
}

if (require.main === module) {
  main();
}

module.exports = { packCrx, buildZip };
