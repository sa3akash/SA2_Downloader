#!/usr/bin/env python3
"""Stage ready-to-load Chromium and Firefox extensions on any desktop OS.

Generates:
- Single drag-and-drop CRX file for Chrome / Edge / Brave / Opera (SA2Downloader-Chrome.crx)
- Single drag-and-drop XPI file for Firefox (SA2Downloader-Firefox.xpi)
- Unpacked folders (browser-extension, browser-extension-firefox)
- Standalone zip archive (SA2Downloader-Chrome.zip)
"""
import argparse
import json
from pathlib import Path
import shutil
import struct
import subprocess
import zipfile

def encode_varint(n):
    res = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            res.append(b | 0x80)
        else:
            res.append(b)
            break
    return bytes(res)

def create_crx3_bytes(zip_bytes, key_path):
    # Try Python cryptography first
    try:
        from cryptography.hazmat.primitives.asymmetric import padding
        from cryptography.hazmat.primitives import hashes, serialization
        with open(key_path, 'rb') as f:
            priv = serialization.load_pem_private_key(f.read(), password=None)
        pub = priv.public_key()
        pub_der = pub.public_bytes(
            encoding=serialization.Encoding.DER,
            format=serialization.PublicFormat.SubjectPublicKeyInfo
        )
        hasher = hashes.Hash(hashes.SHA256())
        hasher.update(pub_der)
        crx_id = hasher.finalize()[:16]

        signed_header_data = bytes([0x0A, 0x10]) + crx_id
        len_buf = struct.pack("<I", len(signed_header_data))
        to_sign = b"CRX3 SignedData\x00" + len_buf + signed_header_data + zip_bytes

        sig = priv.sign(to_sign, padding.PKCS1v15(), hashes.SHA256())

        proof = (
            bytes([0x0A]) + encode_varint(len(pub_der)) + pub_der +
            bytes([0x12]) + encode_varint(len(sig)) + sig
        )
        header = (
            bytes([0x12]) + encode_varint(len(proof)) + proof +
            bytes([0x82, 0xF1, 0x04]) + encode_varint(len(signed_header_data)) + signed_header_data
        )
        header_len = struct.pack("<I", len(header))
        version_buf = struct.pack("<I", 3)
        return b"Cr24" + version_buf + header_len + header + zip_bytes
    except Exception:
        pass
    return None

def stage_extensions(repo, destination, portable=False):
    source = repo / 'browser-extension'
    key_path = source / 'key.pem'
    if not key_path.exists():
        key_path = repo / 'scripts' / 'extension_key.pem'

    ignore = shutil.ignore_patterns('*.test.js', '*.md', 'testdata', '*.ps1', '*.pem', 'key.pem', 'manifest-firefox.json')
    chrome = destination / 'browser-extension'
    shutil.copytree(source, chrome, ignore=ignore, dirs_exist_ok=True)

    version = json.loads((chrome / 'manifest.json').read_text(encoding='utf-8')).get('version', '1.2.1')

    # Chrome CRX3 package (drag & drop into chrome://extensions with Developer mode ON)
    crx = destination / f'SA2Downloader-Chrome-{version}.crx'
    crx_std = destination / 'SA2Downloader-Chrome.crx'

    # Build zip in memory for CRX packaging
    import io
    zip_buf = io.BytesIO()
    with zipfile.ZipFile(zip_buf, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(chrome.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(chrome))
    zip_bytes = zip_buf.getvalue()

    crx_bytes = None
    if key_path.exists():
        crx_bytes = create_crx3_bytes(zip_bytes, key_path)

    # If Python cryptography was unavailable, fall back to bun or node pack_crx.js
    if not crx_bytes and (repo / 'scripts' / 'pack_crx.js').exists():
        for runner in ['bun', 'node']:
            try:
                subprocess.run([runner, str(repo / 'scripts' / 'pack_crx.js'), str(destination)],
                               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if crx_std.exists():
                    crx_bytes = crx_std.read_bytes()
                    break
                elif crx.exists():
                    crx_bytes = crx.read_bytes()
                    break
            except Exception:
                continue

    if crx_bytes:
        crx_std.write_bytes(crx_bytes)
        if not portable:
            crx.write_bytes(crx_bytes)
            shutil.copy2(crx_std, destination / 'sa2_downloader-chrome.crx')

    bat_content = (
        "@echo off\r\n"
        "title SA2 Downloader - Browser Extension Installer\r\n"
        "echo ==========================================================\r\n"
        "echo   SA2 Downloader - Browser Extension Installer (IDM Style)\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "echo Enabling browser extension policy (prevents off-store block)...\r\n"
        "reg add \"HKCU\\Software\\Policies\\Google\\Chrome\\ExtensionInstallAllowlist\" /v 1 /t REG_SZ /d \"lekpmkgkkhcfocbigmdjnoalfnidjlal\" /f >nul 2>&1\r\n"
        "reg add \"HKCU\\Software\\Policies\\Google\\Chrome\\ExtensionInstallSources\" /v 1 /t REG_SZ /d \"<all_urls>\" /f >nul 2>&1\r\n"
        "reg add \"HKCU\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallAllowlist\" /v 1 /t REG_SZ /d \"lekpmkgkkhcfocbigmdjnoalfnidjlal\" /f >nul 2>&1\r\n"
        "reg add \"HKCU\\Software\\Policies\\Microsoft\\Edge\\ExtensionInstallSources\" /v 1 /t REG_SZ /d \"<all_urls>\" /f >nul 2>&1\r\n"
        "reg add \"HKCU\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallAllowlist\" /v 1 /t REG_SZ /d \"lekpmkgkkhcfocbigmdjnoalfnidjlal\" /f >nul 2>&1\r\n"
        "reg add \"HKCU\\Software\\Policies\\BraveSoftware\\Brave-Browser\\ExtensionInstallSources\" /v 1 /t REG_SZ /d \"<all_urls>\" /f >nul 2>&1\r\n"
        "echo Done!\r\n"
        "echo.\r\n"
        "echo Opening Chrome Extensions page...\r\n"
        "start chrome \"chrome://extensions\"\r\n"
        "echo.\r\n"
        "echo INSTALLATION OPTIONS (Both work!):\r\n"
        "echo ----------------------------------------------------------\r\n"
        "echo Option 1 (Drag and Drop - IDM Style):\r\n"
        "echo   - Turn ON \"Developer mode\" toggle in the top-right corner.\r\n"
        "echo   - Drag and drop \"SA2Downloader-Chrome.crx\" into Chrome.\r\n"
        "echo.\r\n"
        "echo Option 2 (Load Unpacked):\r\n"
        "echo   - Click \"Load unpacked\" button in the top-left corner.\r\n"
        "echo   - Select the \"browser-extension\" folder in this directory.\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "pause\r\n"
    )
    (destination / 'install_chrome_extension.bat').write_text(bat_content, encoding='utf-8')

    if portable:
        return [chrome, crx_std]

    # Non-portable (for full release dist): also generate Firefox XPI and Chrome ZIP
    firefox = destination / 'browser-extension-firefox'
    shutil.copytree(source, firefox, ignore=ignore, dirs_exist_ok=True)
    shutil.copy2(source / 'manifest-firefox.json', firefox / 'manifest.json')
    (firefox / 'manifest-firefox.json').unlink(missing_ok=True)
    for chrome_only in ['offscreen.html', 'offscreen.js']:
        (firefox / chrome_only).unlink(missing_ok=True)

    xpi = destination / f'SA2Downloader-Firefox-{version}.xpi'
    xpi_std = destination / 'SA2Downloader-Firefox.xpi'
    with zipfile.ZipFile(xpi, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(firefox.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(firefox))
    shutil.copy2(xpi, xpi_std)
    shutil.copy2(xpi, destination / 'sa2_downloader-firefox.xpi')

    chrome_zip = destination / f'SA2Downloader-Chrome-{version}.zip'
    chrome_zip_std = destination / 'SA2Downloader-Chrome.zip'
    with zipfile.ZipFile(chrome_zip, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(chrome.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(chrome))
    shutil.copy2(chrome_zip, chrome_zip_std)

    return [chrome, firefox, xpi, xpi_std, crx, crx_std, chrome_zip]

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=Path('dist'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for path in stage_extensions(Path(__file__).resolve().parents[1], args.output):
        print(path.resolve())
