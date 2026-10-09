"""Every hosted API symbol stays reachable through stable navigation branches."""
import importlib.util
from pathlib import Path
import re
import unittest


SPEC = importlib.util.spec_from_file_location(
    'hosted_navigation', Path(__file__).resolve().parents[1] / 'docs/hosted/ravet_navigation.py')
navigation = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(navigation)


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


if __name__ == '__main__':
    unittest.main()
