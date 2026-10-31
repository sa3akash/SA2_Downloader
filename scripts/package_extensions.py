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

def stage_extensions(repo, destination):
    source = repo / 'browser-extension'
    key_path = source / 'key.pem'
    if not key_path.exists():
        key_path = repo / 'scripts' / 'extension_key.pem'

    ignore = shutil.ignore_patterns('*.test.js', '*.md', 'testdata', '*.ps1', '*.pem', 'key.pem')
    chrome = destination / 'browser-extension'
    firefox = destination / 'browser-extension-firefox'

    for folder in [chrome, firefox]:
        shutil.copytree(source, folder, ignore=ignore, dirs_exist_ok=True)

    shutil.copy2(source / 'manifest-firefox.json', firefox / 'manifest.json')
    (firefox / 'manifest-firefox.json').unlink(missing_ok=True)
    for chrome_only in ['offscreen.html', 'offscreen.js']:
        (firefox / chrome_only).unlink(missing_ok=True)

    version = json.loads((chrome / 'manifest.json').read_text(encoding='utf-8')).get('version', '1.2.1')

    # Firefox XPI package (drag & drop into Firefox)
    xpi = destination / f'SA2Downloader-Firefox-{version}.xpi'
    xpi_std = destination / 'SA2Downloader-Firefox.xpi'
    with zipfile.ZipFile(xpi, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(firefox.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(firefox))
    shutil.copy2(xpi, xpi_std)

    # Chrome zip package (files placed at root so extraction is clean)
    chrome_zip = destination / f'SA2Downloader-Chrome-{version}.zip'
    chrome_zip_std = destination / 'SA2Downloader-Chrome.zip'
    with zipfile.ZipFile(chrome_zip, 'w', zipfile.ZIP_DEFLATED) as z:
        for f in sorted(chrome.rglob('*')):
            if f.is_file():
                z.write(f, f.relative_to(chrome))
    shutil.copy2(chrome_zip, chrome_zip_std)

    # Chrome CRX3 package (drag & drop into chrome://extensions with Developer mode ON)
    crx = destination / f'SA2Downloader-Chrome-{version}.crx'
    crx_std = destination / 'SA2Downloader-Chrome.crx'

    zip_bytes = chrome_zip.read_bytes()
    crx_bytes = None
    if key_path.exists():
        crx_bytes = create_crx3_bytes(zip_bytes, key_path)

    # If Python cryptography was unavailable, fall back to bun or node pack_crx.js
    if not crx_bytes and (repo / 'scripts' / 'pack_crx.js').exists():
        for runner in ['bun', 'node']:
            try:
                subprocess.run([runner, str(repo / 'scripts' / 'pack_crx.js'), str(destination)],
                               check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                if crx.exists():
                    crx_bytes = crx.read_bytes()
                    break
            except Exception:
                continue

    if crx_bytes:
        crx.write_bytes(crx_bytes)
        crx_std.write_bytes(crx_bytes)
        shutil.copy2(crx, destination / 'sa2_downloader-chrome.crx')
        shutil.copy2(xpi, destination / 'sa2_downloader-firefox.xpi')

    return [chrome, firefox, xpi, xpi_std, crx, crx_std, chrome_zip]

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, default=Path('dist'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for path in stage_extensions(Path(__file__).resolve().parents[1], args.output):
        print(path.resolve())
