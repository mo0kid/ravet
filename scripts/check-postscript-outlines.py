#!/usr/bin/env python3
"""Qualify exact selected PostScript fonts in owned macOS fixtures, never install permanently."""
import argparse, hashlib, json, os, shlex, subprocess, sys, tomllib
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument('--output', type=Path, default=Path('target/postscript-outlines'))
    p.add_argument('--target-dir', type=Path)
    p.add_argument('--consumer-manifest', type=Path)
    p.add_argument('--native-tests', type=Path)
    p.add_argument('--native-include', type=Path)
    p.add_argument('--fixtures', type=Path)
    p.add_argument('--html', type=Path)
    p.add_argument('--cargo-driver', default='cargo')
    p.add_argument('--runner', default='')
    a = p.parse_args()
    if sys.platform != 'darwin':
        raise RuntimeError('macOS own-process qualification; portable Cargo contracts are separate')
    root = a.root.resolve(); out = a.output.resolve(); out.mkdir(parents=True, exist_ok=True)
    global evidence_output
    evidence_output = out
    cache = (a.target_dir or out / 'cargo').resolve()
    tests = (a.native_tests or root / 'crates/ravet-webview/native/tests').resolve()
    fixtures = (a.fixtures or root / 'crates/ravet-surface/tests/fixtures/postscript').resolve()
    html = (a.html or root / 'examples/tutorials/src/postscript_fonts/index.html').resolve()
    environment = dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(cache))
    driver = shlex.split(a.cargo_driver); runner = shlex.split(a.runner)
    records = []
    paths = [Path(__file__).resolve(), html, *fixtures.glob('*')]
    paths += [q for q in tests.glob('postscript-*') if q.is_file()]
    paths += [tests/'compare-postscript-glyphs.py', tests/'compare-postscript-authored.py']
    paths += list((tests/'postscript-consumer/src').rglob('*.rs'))
    for directory in ['crates/ravet-surface/src','crates/ravet-webview/src','crates/ravet-webview/native','examples/tutorials/src/postscript_fonts']:
        paths += [q for q in (root/directory).rglob('*') if q.is_file()]
    paths += [tests/'postscript-browser-host.js']
    paths += [root/q for q in ['crates/ravet-surface/Cargo.toml','crates/ravet-surface/src/text.rs', 'crates/ravet-surface/src/text/postscript.rs', 'crates/ravet-webview/Cargo.toml', 'crates/ravet-webview/native/SystemFonts.mm', 'crates/ravet-webview/native/SystemFonts.h', 'Cargo.lock']]
    if a.consumer_manifest:
        manifest = a.consumer_manifest.resolve()
        paths += [manifest, manifest.parent/'Cargo.lock']
        for dependency in tomllib.loads(manifest.read_text()).get('dependencies', {}).values():
            if isinstance(dependency, dict) and 'path' in dependency:
                package_root = (manifest.parent/dependency['path']).resolve()
                paths += [package_root/'Cargo.toml', package_root/'Cargo.lock', package_root/'build.rs']
                for name in ['src', 'native', 'web', 'tests']:
                    paths += [q for q in (package_root/name).rglob('*') if q.is_file()]
    sources = {str(q): hashlib.sha256(q.read_bytes()).hexdigest() for q in paths if q.is_file()}

    def run(name, argv, stdout=None, combined=False):
        log = out/(name+'.log')
        r = subprocess.run(argv, cwd=root, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        log.write_text(r.stdout+r.stderr)
        if stdout: (out/stdout).write_text(r.stdout)
        records.append(dict(name=name, argv=argv, returncode=r.returncode, log=str(log)))
        (out/'terminal.json').write_text(json.dumps(dict(terminal=bool(r.returncode), commands=records, sources=sources, environment={k:environment[k] for k in ['CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_TARGET_DIR']}), indent=2)+'\n')
        print(name, r.returncode, flush=True)
        if r.returncode: raise RuntimeError(f'{name} refused; preserved {log}')
        return r.stdout+r.stderr if combined else r.stdout

    package = ['--manifest-path',str(a.consumer_manifest.resolve())] if a.consumer_manifest else ['-p','ravet-examples']
    output = run('normal-consumer-build', [*driver,'build',*package,'--locked','--jobs','1','--features','postscript-fonts,live-ui','--bin','postscript_fonts','--message-format=json'])
    artifacts = {}
    for line in output.splitlines():
        try: message=json.loads(line)
        except json.JSONDecodeError: continue
        if message.get('reason')=='compiler-artifact':
            for f in message.get('filenames',[]):
                if f.endswith('.rlib'): artifacts[message['target']['name']]=f
    if not all(name in artifacts for name in ['ravet_surface','ravet_webview']):
        raise RuntimeError('exact dependency artifacts absent from Cargo build receipt')
    binary=cache/'debug/postscript_fonts'
    common=['clang++','-std=c++17','-O2','-fobjc-arc','-fobjc-arc-exceptions','-Wall','-Wextra','-Werror','-mmacosx-version-min=12.0']
    if a.native_include: common += ['-I'+str(a.native_include.resolve())]
    frameworks=['-framework','AppKit','-framework','CoreText']
    for enabled in [0,1]:
        exe=out/f'loader-{enabled}'
        run(f'loader-{enabled}-build',[*common,f'-DRAVET_POSTSCRIPT_FONTS={enabled}',str(tests/'postscript-font-loader.mm'),*frameworks,'-o',str(exe)])
        receipt=run(f'loader-{enabled}',[*runner,str(exe),str(fixtures/'analytic-cff.otf'),str(fixtures/'analytic-cff2.otf')])
        if receipt.count('first-party process unregister=1') != 2 or 'unregister=0' in receipt:
            raise RuntimeError('process-scoped authored registration cleanup not demonstrated')
    for source,name in [('postscript-font-glyphs.mm','system-native'),('postscript-authored-coretext.mm','authored-coretext')]:
        exe=out/name;run(name+'-build',[*common,str(tests/source),*frameworks,'-o',str(exe)])
    run('system-native',[*runner,str(out/'system-native')], 'system-native-glyph-oracle.tsv')
    run('system-rust',[*runner,str(binary),'--glyph-oracle'],'system-rust-glyph-oracle.log')
    run('system-compare',[sys.executable,str(tests/'compare-postscript-glyphs.py'),str(out)],'system-glyph-comparison.json')
    flags=shlex.split(run('native-backend-flags',['pkg-config','--cflags','--libs','harfbuzz','freetype2']).strip())
    run('authored-backends-build',['clang','-std=c11','-O2','-Wall','-Wextra','-Werror',str(tests/'postscript-authored-outlines.c'),*flags,'-o',str(out/'authored-backends')])
    object_file=out/'registration.o'
    run('registration-helper-build',[*common,'-c',str(tests/'postscript-fixture-registration.mm'),'-o',str(object_file)])
    rust=['rustc','--edition=2024','-D','warnings','-L','dependency='+str(cache/'debug/deps')]
    for name in ['ravet_surface','ravet_webview']: rust += ['--extern',name+'='+artifacts[name]]
    run('authored-rust-build',[*rust,str(tests/'postscript-consumer/src/bin/authored.rs'),'-o',str(out/'authored-rust')])
    run('authored-window-build',[*rust,str(tests/'postscript-consumer/src/main.rs'),'-C','link-arg='+str(object_file),'-o',str(out/'authored-window')])
    files=[str(fixtures/q) for q in ['analytic-cff.otf','subr-cff.otf','matrix-cff.otf','tiny-positive-cff.otf','tiny-negative-cff.otf','tiny-negative-matrix-cff.otf','analytic-cff2.otf']]
    run('authored-rust',[*runner,str(out/'authored-rust'),*files],'authored-rust-outline-oracle-final.tsv')
    run('authored-backends',[str(out/'authored-backends'),*files],'authored-native-outline-oracle-ordered.tsv')
    run('authored-coretext',[*runner,str(out/'authored-coretext'),*files],'authored-coretext-outline-oracle.tsv')
    run('authored-compare',[sys.executable,str(tests/'compare-postscript-authored.py'),str(out)],'authored-engine-comparison.json')
    run('normal-window',[*runner,str(binary),'--fixture'])
    receipt=run('authored-window',[*runner,str(out/'authored-window'),str(fixtures/'consumer-cff2.otf')],combined=True)
    if 'fixture process unregister=true' not in receipt:
        raise RuntimeError('authored consumer cleanup receipt absent')
    states=json.loads(run('export-first-party',[*runner,str(binary),'--export-first-party'],'cff-first-party-states.json'))
    if len(states)!=60: raise RuntimeError('expected 60 bounded authored render states')
    host=(tests/'postscript-browser-host.js').read_text()
    for index in [0,1]:
        path=out/f'browser-{index+1}.html'
        document=html.read_text().replace('</body>','<script>'+host.replace('FIXTURE_STATES',json.dumps(states[index*30:(index+1)*30],separators=(',',':')))+'</script></body>')
        if len(document.encode())>1024*1024:raise RuntimeError('browser fixture exceeds native document bound')
        path.write_text(document);run(f'webkit-{index+1}',[*runner,str(binary),'--browser-fixture',str(path)])
    if any(hashlib.sha256(Path(path).read_bytes()).hexdigest()!=digest for path,digest in sources.items()):
        raise RuntimeError('qualified source changed during own-process evidence')
    (out/'terminal.json').write_text(json.dumps(dict(terminal=True,success=True,commands=records,sources=sources,environment={k:environment[k] for k in ['CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_TARGET_DIR']}),indent=2)+'\n')
    print('Exact first-party/native receipts complete; Chromium remains a separate trusted product-tool receipt.',flush=True)

if __name__=='__main__':
    evidence_output = None
    try:
        main()
    except Exception as error:
        if evidence_output is not None:
            terminal = evidence_output/'terminal.json'
            receipt = json.loads(terminal.read_text()) if terminal.is_file() else {}
            receipt.update(terminal=True, success=False, diagnostic=str(error))
            terminal.write_text(json.dumps(receipt, indent=2)+'\n')
        raise
