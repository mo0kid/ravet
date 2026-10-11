"""Documentation navigation as shared data, plus its reference HTML rendering.

The hosted build writes the complete tree once to ``docs/nav-<hash>.json``. Each
page carries only a small static fallback; the website script renders the rest.
This module uses only the standard library.
"""
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


def link_node(url, label, title=None, symbol=False):
    """A navigation link. ``title`` becomes a tooltip; ``symbol`` uses code type."""
    node = {'h': url, 'l': label}
    if title is not None:
        node['t'] = title
    if symbol:
        node['s'] = 1
    return node


def branch_node(nav_id, summary, children, default_open=False, lazy=False):
    """A collapsible branch with a stable ``data-nav-id``.

    ``summary`` is plain text or a link node. A branch opens when
    ``default_open`` is set or when its children (not its own summary) lead to
    the current page. Closed ``lazy`` branches are left out of each page's
    static fallback; the website script renders them from the shared file.
    """
    node = {'id': nav_id, 'm': summary, 'c': children}
    if default_open:
        node['o'] = 1
    if lazy:
        node['z'] = 1
    return node


def api_nodes(tree):
    """Return the API reference tree as navigation nodes with stable IDs."""
    def identifier(prefix, value):
        return prefix + re.sub(r'[^\w-]+', '-', str(value)).strip('-')

    def link(item, label=None, symbol=False):
        return link_node(item['url'], label or item['title'].split('::')[-1], item['title'], symbol)

    def module(node):
        page = node['page']
        contents = [link(page, 'Overview')]
        for child in sorted(node['modules'], key=lambda n: n['page']['title'].lower()):
            contents.append(module(child))
        kinds = list(KINDS) + sorted(set(node['symbols']) - set(KINDS))
        for kind in kinds:
            symbols = sorted(node['symbols'].get(kind, []), key=lambda s: s['title'].lower())
            if not symbols:
                continue
            entries = []
            for symbol in symbols:
                entry = link(symbol, symbol=True)
                methods = symbol['methods']
                if methods:
                    entry = branch_node(identifier('api-symbol-', symbol['path']), entry,
                                        [link(method, symbol=True) for method in methods], lazy=True)
                entries.append(entry)
            contents.append(branch_node(identifier('api-kind-', str(page['path'].parent)+'-'+kind),
                                        KINDS.get(kind, kind.title()), entries, lazy=True))
        nav_id = 'crate-'+page['crate'] if page['kind'] == 'crate' else identifier('api-module-', page['path'].parent)
        return branch_node(nav_id, page['title'].split('::')[-1], contents, lazy=True)

    return [link_node('/ravet/docs/api/', 'Reference overview')] + \
        [module(node) for node in sorted(tree, key=lambda n: n['page']['title'])]


def leads_to(nodes, current):
    """Whether any node, including a branch's link summary, links to ``current``."""
    for node in nodes:
        if 'h' in node:
            if node['h'] == current:
                return True
        elif (isinstance(node['m'], dict) and node['m']['h'] == current) or leads_to(node['c'], current):
            return True
    return False


def render_nodes(nodes, current, complete=False):
    """Render navigation nodes as the sidebar markup used by the website.

    With ``complete`` every branch is rendered, open where it leads to
    ``current``; the website script produces the same markup from the shared
    file. Otherwise this is a page's static fallback for readers without
    JavaScript: closed lazy branches are omitted, except that a branch whose
    summary is a link (an API type with methods) keeps that link.
    """
    def render(node):
        if 'h' in node:
            title = f' title="{html.escape(node["t"], quote=True)}"' if 't' in node else ''
            style = ' class="nav-symbol"' if node.get('s') else ''
            active = ' aria-current="page"' if node['h'] == current else ''
            return f'<a href="{html.escape(node["h"], quote=True)}"{title}{style}{active}>{html.escape(node["l"])}</a>'
        opened = bool(node.get('o')) or leads_to(node['c'], current)
        if node.get('z') and not opened and not complete:
            # Static fallback: keep a symbol's own link, drop its closed contents.
            return render(node['m']) if isinstance(node['m'], dict) else ''
        summary = render(node['m']) if isinstance(node['m'], dict) else html.escape(node['m'])
        body = ''.join(render(child) for child in node['c'])
        return (f'<details data-nav-id="{html.escape(node["id"], quote=True)}"' + (' open' if opened else '') +
                f'><summary>{summary}</summary><div class="nav-branch">{body}</div></details>')
    return ''.join(render(node) for node in nodes)


def navigation_ids(nodes):
    """Every branch ID in document order."""
    ids = []
    for node in nodes:
        if 'id' in node:
            ids.append(node['id'])
            ids.extend(navigation_ids(node['c']))
    return ids


def navigation_links(nodes):
    """Every link URL, including link summaries, in document order."""
    urls = []
    for node in nodes:
        if 'h' in node:
            urls.append(node['h'])
        else:
            if isinstance(node['m'], dict):
                urls.append(node['m']['h'])
            urls.extend(navigation_links(node['c']))
    return urls


def api_bookmarks(tree, current):
    """Complete API bookmarks for ``current``, as rendered by the website script."""
    return render_nodes(api_nodes(tree), current, complete=True)
