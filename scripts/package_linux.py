#!/usr/bin/env python3
"""Build native DEB/RPM and a type-2 AppImage from one glibc-baseline binary.
Requires dpkg-deb, rpmbuild, mksquashfs, and the AppImage type2 runtime.
"""
import argparse
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from package_extensions import stage_extensions

REPO=Path(__file__).resolve().parents[1]
VERSION=(REPO/'VERSION').read_text().strip()
DESKTOP='''[Desktop Entry]
Type=Application
Name=Download Craft
Comment=Download manager with browser capture
Exec=sa2_download
Icon=com.SA2Downloader.app
Terminal=false
Categories=Network;FileTransfer;
StartupNotify=true
'''

def install_tree(root,binary):
    app=root/'usr/lib/SA2Downloader';app.mkdir(parents=True)
    shutil.copy2(binary,app/'sa2_download');(app/'sa2_download').chmod(0o755)
    stage_extensions(REPO,app)
    bindir=root/'usr/bin';bindir.mkdir(parents=True)
    (bindir/'sa2_download').symlink_to('../lib/SA2Downloader/sa2_download')
    desktop=root/'usr/share/applications/com.SA2Downloader.app.desktop';desktop.parent.mkdir(parents=True);desktop.write_text(DESKTOP)
    icon=root/'usr/share/icons/hicolor/256x256/apps/com.SA2Downloader.app.png';icon.parent.mkdir(parents=True);shutil.copy2(REPO/'assets/icon.png',icon)
    docs=root/'usr/share/doc/SA2Downloader';docs.mkdir(parents=True)
    for name in ['LICENSE','PORTABLE-BUILDS.md','RELEASE-NOTES.md']:shutil.copy2(REPO/name,docs/name)
    for item in (REPO/'assets/fonts').glob('OFL-*.txt'):shutil.copy2(item,docs/item.name)
    shutil.copy2(REPO/'NOTICE.md',docs/'NOTICE.md')
    for item in (REPO/'licenses').glob('*.txt'):shutil.copy2(item,docs/item.name)

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--runtime',type=Path,required=True)
    p.add_argument('--output',type=Path,default=Path('dist'))
    p.add_argument('--rpmbuild',default='rpmbuild')
    p.add_argument('--mksquashfs',default='mksquashfs')
    a=p.parse_args();a.output=a.output.resolve();a.output.mkdir(parents=True,exist_ok=True)
    binary=a.binary.resolve()
    symbols=subprocess.run(['objdump','-T',str(binary)],capture_output=True,text=True,check=True).stdout
    versions={tuple(map(int,v.split('.'))) for v in re.findall(r'GLIBC_([0-9.]+)',symbols)}
    maximum=max(versions)
    if maximum>(2,28):raise SystemExit(f'Binary needs glibc {maximum}; build with cargo zigbuild --target x86_64-unknown-linux-gnu.2.28')
    with tempfile.TemporaryDirectory(prefix='SA2Downloader-linux-') as temp:
        temp=Path(temp);root=temp/'root';install_tree(root,binary)
        control=root/'DEBIAN';control.mkdir()
        (control/'control').write_text(f'''Package: SA2Downloader
Version: {VERSION}
Section: net
Priority: optional
Architecture: amd64
Maintainer: Download Craft contributors <noreply@SA2Downloader.local>
Homepage: https://github.com/hungry-detective/SA2Downloader
Depends: libc6 (>= 2.28), libgcc-s1, libx11-6, libxcb1, libxkbcommon0, libgl1, libwayland-client0, xdg-utils
Recommends: libglib2.0-bin
Description: Download Craft desktop download manager
 Download manager with Chrome, Brave and Firefox integration, media download
 support, resumable transfers and per-user helper installation.
''')
        deb=a.output/f'SA2Downloader_{VERSION}_amd64.deb'
        subprocess.run(['dpkg-deb','--root-owner-group','--build',str(root),str(deb)],check=True)
        shutil.rmtree(control)
        top=temp/'rpm';(top/'SPECS').mkdir(parents=True)
        spec=top/'SPECS/SA2Downloader.spec'
        spec.write_text(f'''Name: SA2Downloader
Version: {VERSION}
Release: 1
Summary: Desktop download manager with browser capture
License: MIT AND OFL-1.1
URL: https://github.com/hungry-detective/SA2Downloader
BuildArch: x86_64
AutoReqProv: no
Requires: glibc >= 2.28, libgcc, libX11, libxcb, libxkbcommon, mesa-libGL, wayland-libs, xdg-utils
%global debug_package %{{nil}}
%global _build_id_links none
%global __os_install_post %{{nil}}
%description
Download Craft desktop download manager with Chrome, Brave and Firefox capture.
%install
mkdir -p %{{buildroot}}
cp -a "{root}/usr" %{{buildroot}}/
%files
/usr/bin/sa2_download
/usr/lib/SA2Downloader
/usr/share/applications/com.SA2Downloader.app.desktop
/usr/share/icons/hicolor/256x256/apps/com.SA2Downloader.app.png
%doc /usr/share/doc/SA2Downloader
''')
        subprocess.run([a.rpmbuild,'--define',f'_topdir {top}','--define','_binary_payload w9.gzdio','-bb',str(spec)],check=True)
        rpm=next((top/'RPMS').rglob('*.rpm'));shutil.copy2(rpm,a.output/rpm.name)
        appdir=temp/'SA2Downloader.AppDir';shutil.copytree(root,appdir,symlinks=True)
        (appdir/'com.SA2Downloader.app.desktop').write_text(DESKTOP)
        shutil.copy2(REPO/'assets/icon.png',appdir/'com.SA2Downloader.app.png')
        (appdir/'.DirIcon').symlink_to('com.SA2Downloader.app.png')
        run=appdir/'AppRun';run.write_text('''#!/bin/sh
APPDIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export APPDIR
exec "$APPDIR/usr/lib/SA2Downloader/sa2_download" "$@"
''');run.chmod(0o755)
        squash=temp/'payload.squashfs'
        subprocess.run([a.mksquashfs,str(appdir),str(squash),'-noappend','-all-root','-no-xattrs','-comp','zstd','-processors','2'],check=True)
        image=a.output/f'SA2Downloader-{VERSION}-x86_64.AppImage'
        with image.open('wb') as out:
            out.write(a.runtime.read_bytes())
            with squash.open('rb') as source:shutil.copyfileobj(source,out)
        image.chmod(0o755)
        print('Built:',deb,a.output/rpm.name,image,sep='\n')

if __name__=='__main__':main()
