"""Build-time syntax highlighting, styled with the Ayu Dark palette in ravet.css."""
import html
from bs4 import BeautifulSoup, NavigableString
from pygments.lexers import get_lexer_by_name
from pygments.token import Token
from pygments.util import ClassNotFound


def token_class(token):
    for family, name in [(Token.Comment, 'comment'), (Token.Literal.String.Doc, 'comment'), (Token.Keyword.Type, 'type'),
                         (Token.Keyword, 'keyword'), (Token.Literal.String, 'string'),
                         (Token.Literal.Number, 'number'), (Token.Name.Function, 'function'),
                         (Token.Name.Class, 'type'), (Token.Name.Builtin, 'type'),
                         (Token.Name.Tag, 'tag'), (Token.Name.Attribute, 'attribute'),
                         (Token.Name.Constant, 'constant'), (Token.Operator, 'operator')]:
        if token in family: return 'syntax-' + name
    return ''


def highlight_lines(source, language):
    try: lexer = get_lexer_by_name(language)
    except ClassNotFound: return [html.escape(line) for line in source.split('\n')]
    # The unprocessed token API preserves exact whitespace and trailing newlines.
    # Lex the whole file so multiline strings/comments keep their state across rows.
    lines = ['']
    for _, token, value in lexer.get_tokens_unprocessed(source):
        css = token_class(token)
        for index, part in enumerate(value.split('\n')):
            if index: lines.append('')
            escaped = html.escape(part)
            lines[-1] += f'<span class="{css}">{escaped}</span>' if css and part else escaped
    return lines


def highlight_code(code, language):
    return '\n'.join(highlight_lines(code, language))


def highlight_blocks(content):
    soup = BeautifulSoup(content, 'html.parser', preserve_whitespace_tags={'pre','textarea','code','span'})
    for node in soup.select('pre code, .api-reference .code-header'):
        classes = node.get('class', []) + (node.parent.get('class', []) if node.parent else [])
        language = next((c.removeprefix('language-') for c in classes if c.startswith('language-')), None)
        if language is None:
            language = 'rust' if node.find_parent(class_='api-reference') else 'text'
        # Preserve Rustdoc's clickable type/function links and existing token spans.
        if node.find('a') or node.find('span'):
            for text in list(node.descendants):
                if not isinstance(text, NavigableString) or text.parent.name == 'span': continue
                fragment = BeautifulSoup('<code>'+highlight_code(str(text), language)+'</code>', 'html.parser', preserve_whitespace_tags={'code','span'}).code
                text.replace_with(*list(fragment.contents))
        else:
            fragment = BeautifulSoup('<code>'+highlight_code(node.get_text(), language)+'</code>', 'html.parser', preserve_whitespace_tags={'code','span'}).code
            node.clear()
            node.extend(list(fragment.contents))
    return str(soup)
