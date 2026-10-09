"""Import public Rustdoc content into the DJW documentation shell, without source pages."""
import hashlib
import html
import json
import os
from pathlib import Path
import shutil
import subprocess
from urllib.parse import unquote, urlsplit
from bs4 import BeautifulSoup


def build_reference(source, api_dir=None):
    if api_dir is None:
        target = Path('/tmp/djw-ravet-reference-build')
        # The dedicated output is disposable; never clean the user's target directory.
        if (target / 'doc').exists():
            shutil.rmtree(target / 'doc')
        subprocess.run(['cargo', 'doc', '--workspace', '--all-features', '--no-deps', '--locked',
                        '--target-dir', str(target)], cwd=source,
                       env=dict(os.environ, RUSTDOCFLAGS='-D warnings'), check=True)
        api_dir = target / 'doc'
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--no-deps', '--format-version', '1', '--locked'], cwd=source, text=True))
    packages = {}
    for package in metadata['packages']:
        for target in package['targets']:
            name = target['name'].replace('-', '_')
            if (api_dir/name/'index.html').exists():
                packages[name] = {'name':package['name'], 'description':package.get('description') or '',
                                  'framework':Path(package['manifest_path']).parent.parent.name == 'crates'}
    pages = {}
    for crate in sorted(packages):
        for path in sorted((api_dir/crate).rglob('*.html')):
            if path.name == 'all.html':
                continue
            soup = BeautifulSoup(path.read_text(), 'html.parser')
            content = soup.select_one('#main-content')
            if content is None:
                continue
            content.name = 'div'
            heading = content.select_one('.main-heading h1')
            if heading is None:
                continue
            for node in content.select('script, button, rustdoc-toolbar, a.src, .out-of-band, .item-info .stab.unstable'):
                node.decompose()
            # Source snapshots and compiler UI assets are deliberately not published.
            for node in content.select('.sub-heading'):
                if not node.get_text(strip=True): node.decompose()
            kind = path.name.split('.')[0] if path.name != 'index.html' else ('crate' if path.parent.name == crate else 'module')
            relative = path.relative_to(api_dir)
            name = path.stem.split('.',1)[-1] if path.name != 'index.html' else path.parent.name
            context = '::'.join(relative.parts[:-1])
            qualified = context if path.name == 'index.html' else context + '::' + name
            pages[path.resolve()] = {'path':relative,'url':'/ravet/docs/api/'+str(relative),
                'title':qualified,'kind':kind,'content':content,'crate':crate,
                'description':content.select_one('.top-doc .docblock')}
    # Normalize directory pages for clean links from guides.
    for item in pages.values():
        if item['path'].name == 'index.html': item['url'] = item['url'][:-10]
    for source_path, item in pages.items():
        for link in list(item['content'].find_all('a', href=True)):
            url = urlsplit(link['href'])
            if url.scheme and url.scheme not in {'http','https','mailto'}:
                link.unwrap(); continue
            if url.scheme or url.netloc or not url.path:
                continue
            destination = (source_path.parent/unquote(url.path)).resolve()
            if destination in pages:
                link['href'] = pages[destination]['url'] + ('#'+url.fragment if url.fragment else '')
            elif url.path.endswith('all.html'):
                link['href'] = '/ravet/docs/api/'
            else:
                # Unbuilt dependencies/private source paths aren't dead local links.
                # External dependency references already emitted by Rustdoc retain their URLs.
                link.unwrap()
        # Small compiler controls rely on Rustdoc JS; retain semantic content only.
        for node in item['content'].select('span.hidden, .tooltip, .sub-heading:empty'):
            node.decompose()
        description = item['description']
        text = item['content'].get_text(' ', strip=True)
        item['search'] = {'title':item['title'],'url':item['url'],'kind':item['kind'],
                          'text':text[:18000]}
        item['summary'] = description.get_text(' ',strip=True)[:360] if description else ''
        item['methods'] = []
        for method in item['content'].select('section.method[id]'):
            if 'trait-impl' in method.get('class',[]):
                continue
            identifier = method['id']
            name = identifier.split('.',1)[-1]
            signature = method.select_one('.code-header')
            details = method.find_parent('details',class_='method-toggle')
            block = details.select_one('.docblock') if details else None
            item['methods'].append({'title':item['title']+'::'+name,'url':item['url']+'#'+identifier,
                'kind':'method','text':(signature.get_text(' ',strip=True) if signature else '') + ' ' +
                (block.get_text(' ',strip=True)[:2500] if block else item['summary'])})
    if not pages:
        raise ValueError('Rustdoc contains no public reference pages')
    digest = hashlib.sha256()
    for path in sorted(source.glob('crates/*/src/**/*.rs')):
        digest.update(str(path.relative_to(source)).encode()); digest.update(path.read_bytes())
    return {'pages':pages,'packages':packages,'sourceHash':digest.hexdigest(),
            'toolchain':subprocess.check_output(['rustc','--version'],text=True).strip()}


def reference_links(reference, crate):
    return sorted((p for p in reference['pages'].values() if p['crate']==crate and p['kind'] in {'crate','module'}),
                  key=lambda p:str(p['path']))


def symbol_link(reference, qualified, label=None):
    for item in reference['pages'].values():
        if item['title'] == qualified:
            return f'<a href="{item["url"]}"><code>{html.escape(label or qualified)}</code></a>'
        for method in item['methods']:
            if method['title'] == qualified:
                return f'<a href="{method["url"]}"><code>{html.escape(label or qualified)}</code></a>'
    raise ValueError(f'Unknown public API: {qualified}')
