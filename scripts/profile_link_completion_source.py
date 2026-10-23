"""Independently authored ordinary-link completion bytes and expected label."""
import json
from profile_empty_canvas_title import ORIGINAL_COMPLETION_BYTES, fallback, require

LABEL='Review &amp; %2F / https://example.test/a%2Fb?x=1&amp;y=2'
FLAG='SUPER_PLATINUM_PROFILE_LINK_TITLE_COMPLETION_CHECK'
COMPLETION_STAGES=('opened','query-ready','inserted','saved')


def blocks():
    return [dict(type='rich_text',block_id='link-completion-title',opaque=dict(keep='block'),elements=[
        dict(type='rich_text_section',opaque=dict(keep='section'),elements=[
            dict(type='link',text='Review &amp; %2F',url='https://example.test/not-the-label',style=dict(bold=True),opaque_link=dict(keep='first')),
            dict(type='text',text=' / '),
            dict(type='link',text='',url='https://example.test/a%2Fb?x=1&amp;y=2',style=dict(italic=True),opaque_link=dict(keep='second'))])])]


def completion_bytes():
    return ORIGINAL_COMPLETION_BYTES.replace('"title":"Alex Canvas"','"title":"Alex Canvas","title_blocks":'+
        json.dumps(blocks(),sort_keys=True,separators=(',',':')),1)


def snapshot(case='completion'):
    require(case=='completion','Unknown link completion case')
    return dict(label=LABEL,blocks=blocks(),completionResponseBytes=completion_bytes())
