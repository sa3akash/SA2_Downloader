#!/usr/bin/env python3
"""Stage ready-to-load Chromium and Firefox extensions on any desktop OS."""
import argparse
import json
from pathlib import Path
import shutil
import zipfile

def stage_extensions(repo, destination):
    source=repo/'browser-extension'
    # Tests, their fixtures and the Firefox build script are not part of the
    # extension; offscreen.* is Chrome-only (Firefox runs that in its
    # background page).
    ignore=shutil.ignore_patterns('*.test.js','*.md','testdata','*.ps1')
    chrome=destination/'browser-extension'
    firefox=destination/'browser-extension-firefox'
    for folder in [chrome,firefox]:
        shutil.copytree(source,folder,ignore=ignore,dirs_exist_ok=True)
    shutil.copy2(source/'manifest-firefox.json',firefox/'manifest.json')
    (firefox/'manifest-firefox.json').unlink(missing_ok=True)
    for chrome_only in ['offscreen.html','offscreen.js']:
        (firefox/chrome_only).unlink(missing_ok=True)
    version=json.loads((firefox/'manifest.json').read_text())['version']
    xpi=destination/f'SA2Downloader-Firefox-{version}.xpi'
    with zipfile.ZipFile(xpi,'w',zipfile.ZIP_DEFLATED) as z:
        for f in sorted(firefox.rglob('*')):
            if f.is_file():z.write(f,f.relative_to(firefox))
    chrome_zip=destination/f'SA2Downloader-Chrome-{version}.zip'
    with zipfile.ZipFile(chrome_zip,'w',zipfile.ZIP_DEFLATED) as z:
        for f in sorted(chrome.rglob('*')):
            if f.is_file():z.write(f,f.relative_to(chrome.parent))
    return chrome,firefox,xpi,chrome_zip

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--output',type=Path,default=Path('dist/extensions'))
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    for path in stage_extensions(Path(__file__).resolve().parents[1],args.output):print(path.resolve())
