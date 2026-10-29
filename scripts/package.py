#!/usr/bin/env python3
"""Package a native cargo release without downloading build dependencies."""
import argparse, json, pathlib, platform, plistlib, shutil, stat, tempfile, zipfile, re, subprocess
from package_extensions import stage_extensions
p=argparse.ArgumentParser()
p.add_argument('--binary', type=pathlib.Path)
p.add_argument('--platform', choices=['windows','linux','macos'])
p.add_argument('--arch', default=platform.machine())
p.add_argument('--portable', action='store_true', help='Append -portable to package name')
p.add_argument('--signer', type=pathlib.Path, help='Optional rcodesign binary for cross-built Mac bundles')
p.add_argument('--output', type=pathlib.Path, default=pathlib.Path('dist'))
a=p.parse_args()
repo=pathlib.Path(__file__).resolve().parents[1]
version=(repo/'VERSION').read_text().strip()
osname=a.platform or {'Windows':'windows','Darwin':'macos','Linux':'linux'}[platform.system()]
cand_names = ['sa2_downloader.exe'] if osname=='windows' else ['sa2_downloader', 'download_craft']
binary = a.binary
if not binary or not binary.is_file():
    for name in cand_names:
        for prefix in [
            repo / 'target/release',
            repo / 'target/x86_64-pc-windows-msvc/release',
            repo / 'target/x86_64-unknown-linux-gnu/release',
            repo / 'target/x86_64-apple-darwin/release',
            repo / 'target/aarch64-apple-darwin/release',
        ]:
            cand = prefix / name
            if cand.is_file():
                binary = cand
                break
        if binary and binary.is_file():
            break
if not binary or not binary.is_file():
    for cand in (repo / 'target/release').glob('*.exe' if osname == 'windows' else '*'):
        if cand.is_file() and not cand.name.endswith(('.d', '.pdb', '.rlib', '.rmeta')):
            binary = cand
            break
if not binary or not binary.is_file():
    binary = repo / 'target/release' / cand_names[0]
if not binary.is_file():
    raise FileNotFoundError(f"Binary not found at '{binary}'. Please run 'cargo build --release' first before packaging.")
a.output.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory() as temp:
    suffix = '-portable' if (a.portable or osname == 'windows') else ''
    root = pathlib.Path(temp) / f'SA2Downloader-{version}-{osname}-{a.arch}{suffix}'
    root.mkdir()
    if osname=='macos':
        contents=root/'Download SA2.app/Contents'
        dest=contents/'MacOS/sa2_downloader'
        dest.parent.mkdir(parents=True)
        with (contents/'Info.plist').open('wb') as f:
            plistlib.dump({'CFBundleName':'Download SA2','CFBundleDisplayName':'Download SA2',
                'CFBundleIdentifier':'com.SA2Downloader.app','CFBundleExecutable':'sa2_downloader',
                'CFBundlePackageType':'APPL','CFBundleVersion':version.rsplit('.',1)[0]+version.rsplit('.',1)[1] if version.count('.')==3 else version,'CFBundleShortVersionString':version,
                'LSMinimumSystemVersion':'12.0','NSHighResolutionCapable':True},f)
    else:
        dest=root/binary.name
    shutil.copy2(binary,dest)
    if osname=='windows' and binary.name != 'sa2_downloader.exe':
        shutil.copy2(binary, root/'sa2_downloader.exe')
    if osname!='windows': dest.chmod(0o755)
    stage_extensions(repo,root)
    if osname=='macos':
        resources=contents/'Resources';resources.mkdir(parents=True,exist_ok=True)
        stage_extensions(repo,resources)
    shutil.copy2(repo/'RELEASE-NOTES.md',root/'README.md')
    shutil.copy2(repo/'LICENSE',root/'LICENSE')
    for license_file in (repo/'assets/fonts').glob('OFL-*.txt'):
        shutil.copy2(license_file,root/license_file.name)
    shutil.copy2(repo/'NOTICE.md',root/'NOTICE.md')
    for license_file in (repo/'licenses').glob('*.txt'):
        shutil.copy2(license_file,root/license_file.name)
    if osname=='macos' and platform.system()=='Darwin':
        subprocess.run(['codesign','--force','--deep','--sign','-',str(contents.parent)],check=True)
    elif osname=='macos' and a.signer:
        subprocess.run([str(a.signer),'sign','--digest','sha256','--timestamp-url','none',str(contents.parent)],check=True)
    archive=a.output/(root.name+'.zip')
    with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for f in root.rglob('*'):
            if f.is_file(): z.write(f,f.relative_to(root.parent))
    print(archive.resolve())
