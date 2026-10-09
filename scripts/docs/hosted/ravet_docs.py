"""Build hosted framework guides, section pages, API reference and teaching walkthroughs."""
from collections import Counter, defaultdict
from datetime import date
import html
import json
from pathlib import Path
import re
from urllib.parse import urlsplit, unquote
import markdown
from bs4 import BeautifulSoup
from ravet_api import build_reference, symbol_link
from ravet_navigation import api_tree, api_bookmarks
from ravet_practical_guides import GUIDES
from ravet_tutorials import load_tutorials, tutorial_html
from ravet_highlighting import highlight_blocks
from ravet_intro import INTRODUCTION

EXCLUDED = {'WORK_STATUS.md', 'publication.md', 'documentation-site.md', 'README.md'}
GROUPS = {
    'Getting started':['introduction','getting-started','project-starters','architecture','FEATURES','examples'],
    'Processing & events':['extended-processing','parameters','parameter-attachments','editor-control','control-data'],
    'DSP':['signal-processing','audio-graphs'],
    'Instruments & MIDI':['midi','core-midi-ump','osc','sine-clap','sidechain-clap'],
    'State & files':['observable-state','snapshots','session-state','preset-files','audio-files'],
    'Editors & graphics':['native-editor','surface-drawing','web-controls'],
    'Devices & plug-in formats':['audio-devices','plugin-hosting','host-validation','vst3','audio-unit','aax','lv2'],
    'Runtime & distribution':['runtime-services','runtime-validation','entitlements','maintenance','LICENSING','CONTRIBUTING'],
}
SCRIPT = '<script src="/ravet/docs-search.js" defer></script>'


def build_docs(source, output, write_page, api_dir=None):
    reference = build_reference(source, api_dir)
    reference_tree = api_tree(reference)
    tutorials = load_tutorials(source)
    files = sorted(p for p in (source/'docs').glob('*.md') if p.name not in EXCLUDED)
    files += [source/'LICENSING.md', source/'CONTRIBUTING.md']
    mapping = {p.resolve():f'/ravet/docs/{p.stem}/' for p in files}
    records=[]
    nav=defaultdict(list)
    def add(path,title,content,kind='guide',group=None,children=None):
        path=Path(path)
        url='/ravet/'+str(path)
        if path.name == 'index.html': url=url[:-10]
        item={'path':path,'title':title,'content':content,'kind':kind,'url':url,'children':children or []}
        records.append(item)
        if group: nav[group].append(item)
        return item
    def render_markdown(text, source_file=None):
        text=re.sub(r'\{\{api:([^}]+)\}\}',lambda m:symbol_link(reference,m[1]),text)
        md=markdown.Markdown(extensions=['fenced_code','tables','toc','sane_lists'],extension_configs={'toc':{'permalink':True}})
        soup=BeautifulSoup(md.convert(text),'html.parser')
        if source_file:
            for link in list(soup.find_all(['a','img'])):
                attr='href' if link.name=='a' else 'src'
                if not link.has_attr(attr): continue
                url=urlsplit(link[attr])
                if url.scheme or url.netloc or not url.path or url.path.startswith('/'): continue
                target=(source_file.parent/unquote(url.path)).resolve()
                if target in mapping: destination=mapping[target]
                elif target==(source/'LICENSE').resolve(): destination='/ravet/docs/LICENSE'
                elif 'decisions' in target.parts:
                    # Decision records are no longer part of this documentation site.
                    link.unwrap(); continue
                else:
                    relative=target.relative_to(source)
                    destination='https://github.com/mo0kid/ravet/blob/main/'+str(relative)
                link[attr]=destination+('#'+url.fragment if url.fragment else '')
        return str(soup)
    for path in files:
        text=path.read_text()
        match=re.search(r'^#\s+(.+)',text,re.M)
        title=match[1] if match else path.stem
        content=render_markdown(text,path)
        group=next((g for g,slugs in GROUPS.items() if path.stem in slugs),'Runtime & distribution')
        guide=add(f'docs/{path.stem}/index.html',title,content,group=group)
        # Standalone aspect pages retain the exact source section, not a generated paraphrase.
        soup=BeautifulSoup(content,'html.parser')
        headings=soup.find_all('h2',recursive=False)
        if headings:
            outline='<details class="doc-outline" open><summary>Topics in this guide</summary><ul>'
            for heading in headings:
                identifier=heading.get('id')
                if not identifier: continue
                section_title=heading.get_text(' ',strip=True).removesuffix('¶').strip()
                elements=[str(heading)]
                for sibling in heading.next_siblings:
                    if getattr(sibling,'name',None)=='h2': break
                    elements.append(str(sibling))
                section=BeautifulSoup(''.join(elements),'html.parser')
                first=section.find('h2'); first.name='h1'
                # Fragment-only references belong to the complete guide if outside this section.
                ids={node['id'] for node in section.find_all(id=True)}
                for link in section.find_all('a',href=True):
                    if link['href'].startswith('#') and link['href'][1:] not in ids:
                        link['href']=guide['url']+link['href']
                child=add(f'docs/{path.stem}/{identifier}/index.html',section_title,
                    f'<p class="doc-breadcrumb"><a href="{guide["url"]}">{html.escape(title)}</a></p>'+str(section),'topic')
                guide['children'].append(child)
                outline+=f'<li><a href="{child["url"]}">{html.escape(section_title)}</a></li>'
            outline+='</ul></details>'
            # Insert below the source introduction so every full guide remains readable.
            intro_end=content.find('<h2')
            guide['content']=content[:intro_end]+outline+content[intro_end:]
    for slug,title,group,text in GUIDES:
        add(f'docs/reference/{slug}/index.html',title,render_markdown(text),group=group)
    tutorial_index='<h1>Example tutorials</h1><p>Complete walkthroughs of the teaching package: portable processors, four executable examples, offline helpers, the native UI/audio adapter, the HTML editor and regression tests.</p><p>Every source line is numbered and has an expandable explanation. Chapters describe the signal flow, validation, ownership and behavior before the corresponding code.</p><p>The processors and offline traces work without native devices. The live editor requires macOS and the default <code>live-ui</code> feature. Explicit <code>--render</code> or <code>--offline</code> arguments select the hardware-free entry points.</p><table><thead><tr><th>File</th><th>Walkthrough</th><th>Lines</th></tr></thead><tbody>'
    for item in tutorials:
        imports = sorted(set(re.findall(r'\bravet_[a-z_]+', (source/'examples/tutorials'/item['file']).read_text())) & set(reference['packages']))
        related = '<p class="doc-breadcrumb">API: ' + ' · '.join(f'<a href="/ravet/docs/api/{crate}/">{crate}</a>' for crate in imports) + '</p>' if imports else ''
        add(f'docs/tutorials/{item["slug"]}/index.html',item['title'],tutorial_html(item)+related+'<p><a href="/ravet/docs/tutorials/">All tutorial walkthroughs</a></p>','tutorial', 'Example tutorials')
        tutorial_index+=f'<tr><td><code>{html.escape(item["file"])}</code></td><td><a href="{item["url"]}">{html.escape(item["title"])}</a></td><td>{item["lineCount"]}</td></tr>'
    tutorial_index+='</tbody></table><p><a href="/ravet/docs/examples/">Read the example setup and validation guide</a></p>'
    add('docs/tutorials/index.html','Example tutorials',tutorial_index)
    api_index='<h1>API reference</h1><p>Public modules, types, traits, functions, constants and methods generated from the current Rust source with all optional features enabled. Signatures and examples come from Rustdoc. Source files and compiler assets are not included.</p>'
    for framework,label in [(True,'Framework crates'),(False,'Examples and tools')]:
        api_index+=f'<h2>{label}</h2><ul class="api-crates">'
        for crate,package in sorted(reference['packages'].items()):
            if package['framework']!=framework: continue
            api_index+=f'<li><a href="/ravet/docs/api/{crate}/"><code>{crate}</code></a><p>{html.escape(package["description"])}</p></li>'
        api_index+='</ul>'
    add('docs/api/index.html','API reference',api_index)
    for item in reference['pages'].values():
        add(Path('docs/api')/item['path'],item['title'],
            f'<p class="doc-breadcrumb"><a href="/ravet/docs/api/">API reference</a> / <code>{html.escape(item["title"])}</code></p><div class="api-reference">{item["content"]}</div>',item['kind'])
    def link(item,current):
        active=' aria-current="page"' if item['url']==current else ''
        return f'<a href="{item["url"]}"{active}>{html.escape(item["title"])}</a>'
    def branch(identifier,label,children,current,default=False):
        active=any(item['url']==current or current.startswith(item['url']) for item in children)
        opened=' open' if active or default else ''
        return f'<details data-nav-id="{identifier}"{opened}><summary>{html.escape(label)}</summary><div class="nav-branch">'+''.join(link(item,current) for item in children)+'</div></details>'
    def sidebar(current):
        contents='<a href="/ravet/docs/">Documentation home</a>'
        for group in GROUPS:
            order=GROUPS[group]
            children=sorted(nav[group],key=lambda c:order.index(c['path'].parts[1]) if c['path'].parts[1] in order else len(order))
            identifier=re.sub(r'\W+','-',group.lower()).strip('-')
            inner=''
            for item in children:
                inner+=link(item,current)
                if item['children']:
                    inner+=branch('guide-'+item['path'].parts[1], 'Topics', item['children'],current)
            opened=' open' if group=='Getting started' or any(current.startswith(c['url']) for c in children) else ''
            contents+=f'<details data-nav-id="{identifier}"{opened}><summary>{group}</summary><div class="nav-branch">{inner}</div></details>'
        tutorial_home={'url':'/ravet/docs/tutorials/','title':'Tutorial overview'}
        contents+=branch('tutorials','Example tutorials',[tutorial_home]+nav['Example tutorials'],current)
        api_contents=api_bookmarks(reference_tree,current)
        contents+=f'<details data-nav-id="api"'+(' open' if '/api/' in current else '')+f'><summary>API reference</summary><div class="nav-branch">{api_contents}</div></details>'
        return f'<aside class="docs-sidebar"><label for="doc-search">Search documentation</label><input id="doc-search" class="doc-search" type="search" placeholder="Guide, type or function" autocomplete="off"><div class="doc-results" aria-live="polite"></div><details class="guide-nav" data-nav-id="navigation" open><summary>Browse documentation</summary><nav aria-label="Documentation">{contents}</nav></details></aside>'
    method_count=sum(len(p['methods']) for p in reference['pages'].values())
    lines=sum(item['lineCount'] for item in tutorials)
    introduction=render_markdown(INTRODUCTION)
    add('docs/introduction/index.html','Introduction to RAVET',introduction,group='Getting started')
    home_soup=BeautifulSoup(introduction,'html.parser')
    home_soup.h1.clear()
    home_soup.h1.string='RAVET documentation.'
    home=str(home_soup)
    home+=f'<h2>Documentation reference</h2><p>Browse <a href="/ravet/docs/api/">the API reference</a> for types, function signatures and examples, or <a href="/ravet/docs/tutorials/">the tutorial index</a> for complete source walkthroughs.</p><p class="doc-meta">This snapshot contains {len(reference["pages"])} API pages, {method_count} indexed methods and {lines:,} tutorial source lines across {len(tutorials)} files. Repository access currently requires permission.</p>'
    add('docs/index.html','RAVET documentation',home,'home')
    counts=Counter(item['kind'] for item in records)
    expected=set()
    search=[]
    stamp=date.today().strftime('%-d %B %Y')
    for item in records:
        expected.add(item['path'].relative_to('docs'))
        item['content']=highlight_blocks(item['content'])
        body=f'<main id="main" class="rv-wrap docs-layout">{sidebar(item["url"])}<article class="prose"><div class="doc-meta">Source snapshot · {stamp}</div>{item["content"]}</article></main>'
        write_page(item['path'],item['title'],item['title']+' — RAVET framework documentation.',body,SCRIPT)
        # Tutorial code stays in its own pages; search indexes reviewed prose and chapter headings.
        soup=BeautifulSoup(item['content'],'html.parser')
        for node in soup.select('.line-guide'): node.decompose()
        search.append({'title':item['title'],'url':item['url'],'kind':item['kind'],'text':soup.get_text(' ',strip=True)[:10000]})
    for item in reference['pages'].values(): search.extend(item['methods'])
    docdir=output/'docs'
    (docdir/'LICENSE').write_text((source/'LICENSE').read_text())
    (docdir/'search.json').write_text(json.dumps(search,ensure_ascii=False,separators=(',',':')))
    manifest={'snapshot':date.today().isoformat(),'counts':dict(counts),'apiPages':len(reference['pages']),'methods':method_count,'tutorialLines':lines,'sourceHash':reference['sourceHash'],'toolchain':reference['toolchain'],'features':'all','packages':reference['packages'], 'tutorials':[{k:i[k] for k in ['file','url','lineCount','hash','finalNewline']} for i in tutorials]}
    (docdir/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    for old in docdir.rglob('*.html'):
        if old.relative_to(docdir) not in expected: old.unlink()
    for directory in sorted(docdir.rglob('*'),reverse=True):
        if directory.is_dir() and not any(directory.iterdir()): directory.rmdir()
    print(f'Generated {len(records)} documentation pages, {method_count} methods, {len(tutorials)} tutorials / {lines} lines.')
