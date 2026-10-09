"""Compact, complete API bookmarks for the hosted documentation tree."""
import html
import re
from collections import defaultdict


KINDS = {
    'struct': 'Structs', 'enum': 'Enums', 'trait': 'Traits', 'fn': 'Functions',
    'type': 'Type aliases', 'constant': 'Constants', 'static': 'Statics',
    'union': 'Unions', 'macro': 'Macros',
}


def api_tree(reference):
    modules = {}
    for item in reference['pages'].values():
        if item['kind'] in {'crate', 'module'}:
            modules[item['path'].parent] = {'page': item, 'modules': [], 'symbols': defaultdict(list)}
    for directory, node in modules.items():
        if node['page']['kind'] == 'module':
            modules[directory.parent]['modules'].append(node)
    for item in reference['pages'].values():
        if item['kind'] not in {'crate', 'module'}:
            modules[item['path'].parent]['symbols'][item['kind']].append(item)
    return [modules[item['path'].parent] for item in reference['pages'].values() if item['kind'] == 'crate']


def api_bookmarks(tree, current):
    def identifier(prefix, value):
        return prefix + re.sub(r'[^\w-]+', '-', str(value)).strip('-')

    def link(item, label=None, symbol=False):
        active = ' aria-current="page"' if item['url'] == current else ''
        style = ' class="nav-symbol"' if symbol else ''
        return f'<a href="{html.escape(item["url"], quote=True)}" title="{html.escape(item["title"], quote=True)}"{style}{active}>{html.escape(label or item["title"].split("::")[-1])}</a>'

    def branch(nav_id, label, contents, active=False):
        return f'<details data-nav-id="{nav_id}"'+(' open' if active else '')+f'><summary>{label}</summary><div class="nav-branch">{contents}</div></details>'

    def module(node):
        page = node['page']
        contents = link(page, 'Overview')
        for child in sorted(node['modules'], key=lambda n: n['page']['title'].lower()):
            contents += module(child)
        kinds = list(KINDS) + sorted(set(node['symbols']) - set(KINDS))
        for kind in kinds:
            symbols = sorted(node['symbols'].get(kind, []), key=lambda s: s['title'].lower())
            if not symbols:
                continue
            entries = ''
            for symbol in symbols:
                entry = link(symbol, symbol=True)
                methods = symbol['methods']
                if methods:
                    entry = branch(identifier('api-symbol-', symbol['path']), entry,
                        ''.join(link(method, symbol=True) for method in methods))
                entries += entry
            contents += branch(identifier('api-kind-', str(page['path'].parent)+'-'+kind),
                html.escape(KINDS.get(kind, kind.title())), entries,
                any(symbol['url'] == current for symbol in symbols))
        nav_id = 'crate-'+page['crate'] if page['kind'] == 'crate' else identifier('api-module-', page['path'].parent)
        return branch(nav_id, html.escape(page['title'].split('::')[-1]), contents, current.startswith(page['url']))

    return '<a href="/ravet/docs/api/">Reference overview</a>'+''.join(module(node) for node in sorted(tree, key=lambda n: n['page']['title']))
