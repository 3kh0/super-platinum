"""Exact link metadata; reuse unchanged no-user receipt and Quip assertions."""
from pathlib import Path

LABEL = 'Review &amp; %2F / https://example.test/a%2Fb?x=1&amp;y=2'
_path = Path(__file__).with_name('slack_list_empty_canvas_reply_source.py')
_source = _path.read_text(encoding='utf-8')
assert _source.count("'Untitled'") == 1
# Isolated globals: the baseline module and its callers remain unchanged.
exec(compile(_source.replace("'Untitled'", repr(LABEL)), str(_path), 'exec'), globals())


def blocks():
    return [dict(type='rich_text', block_id='ordinary-link-title', opaque_block=dict(keep=7), elements=[
        dict(type='rich_text_section', elements=[
            dict(type='link', text='Review &amp; %2F', url='https://example.test/not-the-label',
                 style=dict(bold=True), opaque_link=dict(keep='first')),
            dict(type='text', text=' / '),
            dict(type='link', text='', url='https://example.test/a%2Fb?x=1&amp;y=2',
                 style=dict(italic=True), opaque_link=dict(keep='second'))])])]


_response = response


def response(original, plain=False):
    require(plain is False, 'Link lifecycle cannot substitute a plain/empty response')
    return _response(original, False)
