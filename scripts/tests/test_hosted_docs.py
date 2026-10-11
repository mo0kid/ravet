"""Every hosted API symbol stays reachable through stable navigation branches."""
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location(
    'hosted_navigation', Path(__file__).resolve().parents[1] / 'docs/hosted/ravet_navigation.py')
navigation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(navigation)
VERSION_SPEC = importlib.util.spec_from_file_location(
    'hosted_version', Path(__file__).resolve().parents[1] / 'docs/hosted/ravet_version.py')
version = importlib.util.module_from_spec(VERSION_SPEC)
VERSION_SPEC.loader.exec_module(version)


class HostedNavigation(unittest.TestCase):
    def setUp(self):
        self.pages = []
        for path, kind, title in [
            ('demo/index.html', 'crate', 'demo'),
            ('demo/dsp/index.html', 'module', 'demo::dsp'),
            ('demo/dsp/struct.Delay.html', 'struct', 'demo::dsp::Delay'),
            ('demo/dsp/fn.mix.html', 'fn', 'demo::dsp::mix'),
            ('demo/constant.RATE.html', 'constant', 'demo::RATE'),
        ]:
            url = '/ravet/docs/api/'+path
            if path.endswith('index.html'):
                url = url[:-10]
            self.pages.append({'path': Path(path), 'url': url, 'kind': kind,
                               'title': title, 'crate': 'demo', 'methods': []})
        self.pages[2]['methods'] = [{'url': self.pages[2]['url']+'#method.process',
                                     'title': 'demo::dsp::Delay::process'}]
        self.tree = navigation.api_tree({'pages': dict(enumerate(self.pages))})

    def test_all_symbols_and_methods_have_direct_bookmarks(self):
        content = navigation.api_bookmarks(self.tree, '/ravet/docs/api/')
        for page in self.pages:
            self.assertIn(f'href="{page["url"]}"', content)
            for method in page['methods']:
                self.assertIn(f'href="{method["url"]}"', content)
        self.assertIn('>Delay</a>', content)
        self.assertIn('>process</a>', content)

    def test_current_symbol_opens_its_ancestors_and_keeps_ids_stable(self):
        first = navigation.api_bookmarks(self.tree, self.pages[2]['url'])
        other = navigation.api_bookmarks(self.tree, self.pages[3]['url'])
        ids = lambda content: re.findall(r'data-nav-id="([^"]+)"', content)
        self.assertEqual(ids(first), ids(other))
        self.assertEqual(len(ids(first)), len(set(ids(first))))
        self.assertIn('data-nav-id="crate-demo" open', first)
        self.assertIn('data-nav-id="api-module-demo-dsp" open', first)
        self.assertIn('data-nav-id="api-kind-demo-dsp-struct" open', first)
        self.assertIn('class="nav-symbol" aria-current="page">Delay</a>', first)

    def test_shared_tree_lists_every_page_and_method_once_with_stable_ids(self):
        nodes = json.loads(json.dumps(navigation.api_nodes(self.tree)))
        links = navigation.navigation_links(nodes)
        for page in self.pages:
            self.assertEqual(links.count(page['url']), 1, page['url'])
        self.assertIn(self.pages[2]['methods'][0]['url'], links)
        ids = navigation.navigation_ids(nodes)
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(ids, ['crate-demo', 'api-module-demo-dsp', 'api-kind-demo-dsp-struct',
                               'api-symbol-demo-dsp-struct-Delay-html', 'api-kind-demo-dsp-fn',
                               'api-kind-demo-constant'])

    def test_page_fallback_keeps_the_current_path_and_drops_closed_branches(self):
        nodes = navigation.api_nodes(self.tree)
        current = self.pages[2]['url']
        fallback = navigation.render_nodes(nodes, current)
        complete = navigation.render_nodes(nodes, current, complete=True)
        self.assertIn('class="nav-symbol" aria-current="page">Delay</a>', fallback)
        self.assertIn('data-nav-id="api-kind-demo-dsp-struct" open', fallback)
        # A closed type with methods keeps its own link but not its methods.
        self.assertNotIn('api-symbol-demo-dsp-struct-Delay-html', fallback)
        self.assertNotIn('#method.process', fallback)
        self.assertNotIn('api-kind-demo-dsp-fn', fallback)
        self.assertIn('#method.process', complete)
        self.assertIn('data-nav-id="api-kind-demo-dsp-fn">', complete)
        self.assertLess(len(fallback), len(complete))

    def test_branches_open_through_children_not_their_own_link_summary(self):
        nodes = navigation.api_nodes(self.tree)
        self.assertFalse(navigation.leads_to(nodes, '/elsewhere/'))
        complete = navigation.render_nodes(nodes, self.pages[2]['url'], complete=True)
        self.assertIn('data-nav-id="api-symbol-demo-dsp-struct-Delay-html">', complete)
        default = navigation.branch_node('start', 'Start', [navigation.link_node('/a/', 'A')], default_open=True)
        self.assertIn('data-nav-id="start" open', navigation.render_nodes([default], '/b/'))


class HostedVersion(unittest.TestCase):
    MANIFEST = {'snapshot': '2026-10-11', 'sourceHash': 'abc', 'apiPages': 3, 'tutorialLines': 9}

    def test_explicit_commit_is_published_for_deployment_checks(self):
        commit = '0123456789abcdef0123456789abcdef01234567'
        with tempfile.TemporaryDirectory() as directory:
            record = version.write_version(directory, self.MANIFEST, directory,
                                           {'RAVET_DOCS_COMMIT': commit.upper()})
            written = json.loads((Path(directory) / 'version.json').read_text())
        self.assertEqual(record, written)
        self.assertEqual(written['commit'], commit)
        self.assertEqual(written['snapshot'], '2026-10-11')
        self.assertEqual(written['repository'], 'mo0kid/ravet')

    def test_malformed_or_missing_commit_is_not_published(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertIsNone(version.source_revision(directory, {'RAVET_DOCS_COMMIT': 'main; rm -rf /'}))
            # A temporary directory outside any checkout has no revision to report.
            self.assertIsNone(version.source_revision(directory, {}))


if __name__ == '__main__':
    unittest.main()
