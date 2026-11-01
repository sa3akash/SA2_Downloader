#!/usr/bin/env python3
"""Stage ready-to-load Chromium and Firefox extensions on any desktop OS.

Generates clean, separate packages without duplicates:
- Chromium / Chrome / Edge / Brave / Opera:
  - browser-extension-chrome/ (unpacked folder)
  - SA2Downloader-Chrome.crx (single drag-and-drop CRX file)
  - install_chrome_extension.bat (1-click helper)
- Firefox:
  - browser-extension-firefox/ (unpacked folder)
  - SA2Downloader-Firefox.xpi (single drag-and-drop XPI file)
  - install_firefox_extension.bat (1-click helper)
"""
import argparse
import io
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
    # Try Python cryptography dynamically (avoids static linter missing-import errors)
    try:
        import importlib
        padding = importlib.import_module("cryptography.hazmat.primitives.asymmetric.padding")
        hashes = importlib.import_module("cryptography.hazmat.primitives.hashes")
        serialization = importlib.import_module("cryptography.hazmat.primitives.serialization")
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

    # 1. Chrome / Edge / Chromium extension
    chrome = destination / 'browser-extension-chrome'
    shutil.copytree(source, chrome, ignore=ignore, dirs_exist_ok=True)

    # Also keep browser-extension folder for compatibility
    compat_folder = destination / 'browser-extension'
    if compat_folder != chrome:
        shutil.copytree(source, compat_folder, ignore=ignore, dirs_exist_ok=True)

    # 2. Chrome CRX3 package (drag & drop into chrome://extensions with Developer mode ON)
    crx_std = destination / 'SA2Downloader-Chrome.crx'

    zip_buf = io.BytesIO()
    with zipfile.ZipFile(zip_buf, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(chrome.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(chrome))
    zip_bytes = zip_buf.getvalue()

    crx_bytes = None
    if key_path.exists():
        crx_bytes = create_crx3_bytes(zip_bytes, key_path)

    if not crx_bytes and (repo / 'scripts' / 'pack_crx.js').exists():
        for runner in ['bun', 'node']:
            try:
                subprocess.run([runner, str(repo / 'scripts' / 'pack_crx.js'), str(destination)],
                               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if crx_std.exists():
                    crx_bytes = crx_std.read_bytes()
                    break
            except Exception:
                continue

    if crx_bytes:
        crx_std.write_bytes(crx_bytes)

    # 1-Click Chrome Installer Batch Script
    chrome_bat_content = (
        "@echo off\r\n"
        "title SA2 Downloader - Chrome Extension Helper\r\n"
        "echo ==========================================================\r\n"
        "echo   SA2 Downloader - Chrome Extension Installer\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "echo Configuring extension policy allowlist...\r\n"
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
        "echo INSTALLATION STEPS:\r\n"
        "echo   Method 1 (Recommended): Turn ON \"Developer mode\" switch (top-right).\r\n"
        "echo            Drag and drop the \"browser-extension-chrome\" folder into Chrome!\r\n"
        "echo.\r\n"
        "echo   Method 2: Turn ON \"Developer mode\" switch (top-right).\r\n"
        "echo            Drag and drop \"SA2Downloader-Chrome.crx\" into Chrome!\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "pause\r\n"
    )
    (destination / 'install_chrome_extension.bat').write_text(chrome_bat_content, encoding='utf-8')

    # 3. Firefox extension
    firefox = destination / 'browser-extension-firefox'
    shutil.copytree(source, firefox, ignore=ignore, dirs_exist_ok=True)
    if (source / 'manifest-firefox.json').exists():
        shutil.copy2(source / 'manifest-firefox.json', firefox / 'manifest.json')
    (firefox / 'manifest-firefox.json').unlink(missing_ok=True)
    for chrome_only in ['offscreen.html', 'offscreen.js']:
        (firefox / chrome_only).unlink(missing_ok=True)

    # Firefox XPI package (drag & drop onto Firefox)
    xpi_std = destination / 'SA2Downloader-Firefox.xpi'
    with zipfile.ZipFile(xpi_std, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(firefox.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(firefox))

    # 1-Click Firefox Installer Batch Script
    firefox_bat_content = (
        "@echo off\r\n"
        "title SA2 Downloader - Firefox Extension Helper\r\n"
        "echo ==========================================================\r\n"
        "echo   SA2 Downloader - Firefox Extension Installer\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "echo Opening Firefox Add-ons page...\r\n"
        "start firefox \"about:addons\"\r\n"
        "echo.\r\n"
        "echo INSTALLATION STEPS:\r\n"
        "echo   1. Simply drag and drop \"SA2Downloader-Firefox.xpi\" onto Firefox window!\r\n"
        "echo   2. Click \"Add\" when Firefox prompts.\r\n"
        "echo ==========================================================\r\n"
        "echo.\r\n"
        "pause\r\n"
    )
    (destination / 'install_firefox_extension.bat').write_text(firefox_bat_content, encoding='utf-8')

    result = [chrome, firefox, crx_std, xpi_std]
    return result

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=Path('dist'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for path in stage_extensions(Path(__file__).resolve().parents[1], args.output):
        print(path.resolve())
