#!/usr/bin/env python3
"""Build Mac universal + Windows x64 portable files, without running either app."""
import hashlib, os, plistlib, shutil, subprocess, sys, tempfile, zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
VERSION='0.3.0'
DIST=ROOT/'dist'
TOOLCHAIN=ROOT/'.build-tools/llvm-mingw-20260908-ucrt-macos-universal/bin'
TARGETS=['aarch64-apple-darwin','x86_64-apple-darwin','x86_64-pc-windows-gnullvm']

def cmd(*args,env=None):
    print('>', ' '.join(str(x) for x in args),flush=True)
    subprocess.run(list(map(str,args)),cwd=ROOT,env=env,check=True)

def zipdir(source:Path,archive:Path):
    with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=9,allowZip64=True) as output:
        for p in sorted(source.rglob('*')):
            if p.is_file():
                info=zipfile.ZipInfo(str(p.relative_to(source)),date_time=(2026,9,24,0,0,0))
                info.compress_type=zipfile.ZIP_DEFLATED
                info.external_attr=(p.stat().st_mode & 0xffff)<<16
                output.writestr(info,p.read_bytes(),compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)

def notices():
    # Bundle license texts alongside the manifest so portable copies retain notices.
    data=subprocess.check_output([shutil.which('cargo') or str(Path.home()/'.cargo/bin/cargo'),'metadata','--locked','--format-version','1'],cwd=ROOT)
    import json
    packages=json.loads(data)['packages']
    lines=['THIRD-PARTY-NOTICES · LAN Transfer '+VERSION,'','This package includes open-source dependencies. Their package metadata and license files follow.','']
    for p in sorted(packages,key=lambda x:x['name'].lower()):
        if p['name']=='lan-transfer':continue
        lines.append('='*72)
        lines.append(f"{p['name']} {p['version']} | {p.get('license') or 'See upstream license'}")
        lines.append(p.get('repository') or p.get('homepage') or 'https://crates.io/crates/'+p['name'])
        base=Path(p['manifest_path']).parent
        licenses=sorted({f for pattern in ['LICENSE*','LICENCE*','COPYING*','NOTICE*'] for f in base.glob(pattern) if f.is_file() and f.stat().st_size<300_000})
        for f in licenses:
            lines.extend(['',f'[{f.name}]',f.read_text(encoding='utf-8',errors='replace')])
        if not licenses:lines.append('License files: see the upstream package.')
        lines.append('')
    return '\n'.join(lines)+'\n'

def build():
    cargo=shutil.which('cargo') or str(Path.home()/'.cargo/bin/cargo')
    rustup=shutil.which('rustup') or str(Path.home()/'.cargo/bin/rustup')
    if not Path(cargo).exists() or not Path(rustup).exists():raise SystemExit('Rust toolchain is missing.')
    if not TOOLCHAIN.exists():raise SystemExit('LLVM-MinGW must be unpacked under .build-tools before packaging Windows.')
    env=dict(os.environ);env['PATH']=str(TOOLCHAIN)+os.pathsep+env.get('PATH','')
    env['CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER']=str(TOOLCHAIN/'x86_64-w64-mingw32-clang')
    env['WINDRES']=str(TOOLCHAIN/'x86_64-w64-mingw32-windres')
    env['MACOSX_DEPLOYMENT_TARGET']='11.0'
    # Keep local paths (user name, project location) out of the binaries:
    # panic messages embed source paths of every dependency.
    home=str(Path.home())
    remap=[f'--remap-path-prefix={home}/.cargo/registry/src=/cargo',f'--remap-path-prefix={ROOT}=/build',f'--remap-path-prefix={home}=/home']
    env['RUSTFLAGS']=(env.get('RUSTFLAGS','')+' '+' '.join(remap)).strip()
    if "--package-only" not in sys.argv:
        for target in TARGETS:
            cmd(rustup,'target','add',target,env=env)
            cmd(cargo,'build','--release','--locked','--target',target,env=env)
    DIST.mkdir(exist_ok=True)
    notices_text=notices()
    with tempfile.TemporaryDirectory(prefix='lan-transfer-package-') as temp:
        stage=Path(temp)
        mac_root=stage/'macOS';mac_root.mkdir()
        app=mac_root/'邻传.app';executable=app/'Contents/MacOS/lan-transfer'
        executable.parent.mkdir(parents=True)
        resources=app/'Contents/Resources';resources.mkdir(parents=True)
        plist={
            'CFBundleDevelopmentRegion':'zh_CN','CFBundleDisplayName':'邻传','CFBundleName':'邻传',
            'CFBundleExecutable':'lan-transfer','CFBundleIdentifier':'app.lantransfer.desktop',
            'CFBundleShortVersionString':VERSION,'CFBundleVersion':'3','CFBundlePackageType':'APPL',
            'CFBundleIconFile':'AppIcon','LSMinimumSystemVersion':'11.0','LSUIElement':True,
            'NSLocalNetworkUsageDescription':'邻传使用局域网发现附近电脑并在两台电脑之间传输文件。',
            'NSHighResolutionCapable':True,
        }
        (app/'Contents/Info.plist').write_bytes(plistlib.dumps(plist,sort_keys=True))
        shutil.copy2(ROOT/'assets/AppIcon.icns',resources/'AppIcon.icns')
        cmd('lipo','-create',ROOT/'target/aarch64-apple-darwin/release/lan-transfer',ROOT/'target/x86_64-apple-darwin/release/lan-transfer','-output',executable)
        executable.chmod(0o755)
        cmd('codesign','--force','--deep','--sign','-','--timestamp=none',app)
        shutil.copy2(ROOT/'packaging/使用说明.txt',mac_root/'使用说明.txt')
        shutil.copy2(ROOT/'assets/FONT-LICENSE.txt',mac_root/'FONT-LICENSE.txt')
        (mac_root/'THIRD-PARTY-NOTICES.txt').write_text(notices_text)
        mac_zip=DIST/f'LanTransfer-{VERSION}-macOS-universal.zip';zipdir(mac_root,mac_zip)
        dmg=DIST/f'LanTransfer-{VERSION}-macOS-universal.dmg'
        if dmg.exists():dmg.unlink()
        cmd('hdiutil','create','-fs','HFS+','-volname','邻传 LAN Transfer','-srcfolder',mac_root,'-format','UDZO','-ov',dmg)
        win_root=stage/'Windows';win_root.mkdir()
        shutil.copy2(ROOT/'target/x86_64-pc-windows-gnullvm/release/lan-transfer.exe',win_root/'LanTransfer.exe')
        shutil.copy2(TOOLCHAIN.parent/'x86_64-w64-mingw32/bin/libunwind.dll',win_root/'libunwind.dll')
        shutil.copy2(TOOLCHAIN.parent/'LICENSE.TXT',win_root/'LLVM-MINGW-LICENSE.txt')
        shutil.copy2(ROOT/'packaging/使用说明.txt',win_root/'使用说明.txt')
        shutil.copy2(ROOT/'assets/FONT-LICENSE.txt',win_root/'FONT-LICENSE.txt')
        (win_root/'THIRD-PARTY-NOTICES.txt').write_text(notices_text)
        win_zip=DIST/f'LanTransfer-{VERSION}-Windows-x64.zip';zipdir(win_root,win_zip)
    paths=[mac_zip,dmg,win_zip]
    digest=''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in paths)
    (DIST/'SHA256SUMS.txt').write_text(digest)
    for p in paths:print(f'{p}: {p.stat().st_size/1024/1024:.1f} MiB')
    # Refuse to call a build finished if a local path slipped into a binary.
    leaks=[str(b) for b in [ROOT/'target/aarch64-apple-darwin/release/lan-transfer',ROOT/'target/x86_64-apple-darwin/release/lan-transfer',ROOT/'target/x86_64-pc-windows-gnullvm/release/lan-transfer.exe'] if str(Path.home()).encode() in b.read_bytes()]
    if leaks:raise SystemExit('Local home path found in: '+', '.join(leaks))
    print('No local paths in binaries.')
    print('Packaging complete. This script does not run or install apps, or run tests.')

if __name__=='__main__':build()
