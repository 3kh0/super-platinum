"""Portable adversarial tone-display receipts; no native or live IO."""
import copy
from pathlib import Path
import runpy
import unittest

V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_reaction_tones.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_actions.py')))
DIR = Path('/private/list-reaction-tones')
NAMES = ['thumbsup::skin-tone-3', '+1::skin-tone-3', 'handshake::skin-tone-2-6',
         'wave', 'fixture_tone_custom', 'not_real::skin-tone-3']
GLYPHS = ['\U0001f44d\U0001f3fc', '\U0001f44d\U0001f3fc',
          '\U0001faf1\U0001f3fb\u200d\U0001faf2\U0001f3ff', '\U0001f44b', None,
          ':not_real::skin-tone-3:']


def sample(narrow=False):
    fixture, base = BASE['sample'](narrow=narrow)
    first = base['stages'][0]
    original = copy.deepcopy(first['model']['comments']['page'])
    original['messages'][1].update(
        reactions=[dict(name=name, count=1, users=['U1']) for name in NAMES],
        tone_fixture_opaque=dict(keep='original raw names'))
    future = copy.deepcopy(original)
    future['messages'][1]['reactions'][1].update(count=2, users=['U1', 'U_RT_REACTOR'])
    rows = []
    for i, name in enumerate(('draft', 'socket', 'refreshed', 'offline')):
        row = copy.deepcopy(first)
        row.update(stage=name, capture=str(DIR / f'{fixture}-{name}.png'))
        m, g, p = row['model'], row['geometry'], row['proof']
        c = m['comments']
        c.update(page=copy.deepcopy(original if i < 2 else future), read=10 + max(0, i-1)*5,
                 error=V['OFFLINE'] if i == 3 else None)
        rx = copy.deepcopy(future if i else original)['messages'][1]['reactions']
        typed = copy.deepcopy(c['pageMessages'])
        typed[1].update(reactions=rx, tone_fixture_opaque=dict(keep='original raw names'))
        c['pageMessages'] = typed
        projected = [dict(name=r['name'], count=r['count']) for r in rx]
        c['reactions'] = [dict(ts=V['FIRST'], reactions=projected)]
        g['rows'][0]['reactions'] = copy.deepcopy(projected)
        m['toneProbe'] = [dict(name=n, glyph=GLYPHS[j], media=j == 4, count=rx[j]['count'],
                               own=False, available=False) for j, n in enumerate(NAMES)]
        m['toneMedia'] = dict(name='fixture_tone_custom', source='https://fixture.invalid/reaction-tone-custom.png',
                              ready=True, asset='assets/icons/png/icon_16.png', network=False)
        reads = (4, 4, 8, 12)[i]
        template = first['model']['ledger']['requests']
        m['ledger'].update(requests=[dict(copy.deepcopy(r), serial=batch*4+j+1)
                                    for batch in range(reads//4) for j, r in enumerate(template)],
                           completed=reads, replies=reads//4)
        p.update(initialResponse=copy.deepcopy(original), serverResponse=copy.deepcopy(future) if i >= 2 else None,
                 toneEvent=None)
        box = copy.deepcopy(g['editor']['rect'])
        pills = []
        for j, n in enumerate(NAMES):
            glyph = GLYPHS[j]
            pills.append(dict(name=n, tag='BUTTON', type='button', disabled=True, ariaDisabled='true', pressed='false',
                              own=False, count=rx[j]['count'], glyph=glyph,
                              codepoints=[] if glyph is None else [ord(char) for char in glyph],
                              media=dict(src='http://super-platinum-media.emoji/fixture?v=1', alt=':fixture_tone_custom:',
                                         complete=True, naturalWidth=16) if j == 4 else None,
                              visible=True, hit=True, rect=copy.deepcopy(box), glyphVisible=True, glyphHit=True,
                              glyphRect=copy.deepcopy(box)))
        g['tones'] = dict(pills=pills, tools=0, notices=[])
        g['action'] = dict(editorText=g['action']['editorText'])
        if i == 3:
            g['notices'] = [V['OFFLINE']]
            g['tones']['notices'] = [dict(text=V['OFFLINE'], visible=True, rect=copy.deepcopy(box))]
        rows.append(row)
    event = dict(team='T1', generation=7, parsed=True,
                 frame=dict(type='reaction_added', user='U_RT_REACTOR', reaction='+1::skin-tone-3',
                            item=dict(type='message', channel='CCOMMENT01', ts='1781524801.000001')),
                 before=copy.deepcopy(rows[0]['model']['toneProbe']), after=copy.deepcopy(rows[1]['model']['toneProbe']))
    for row in rows[1:]: row['proof']['toneEvent'] = copy.deepcopy(event)
    base.update(case='reaction-tones', stages=rows, final=rows[-1]['model'])
    return fixture, base


class ToneReceipts(unittest.TestCase):
    def validate(self, receipt=None, narrow=False):
        fixture, base = sample(narrow)
        return V['validate'](fixture, base if receipt is None else receipt, DIR, png=False)

    def reject(self, mutate, index=1):
        _, r = sample()
        mutate(r['stages'][index])
        with self.assertRaises(RuntimeError): self.validate(r)

    def test_both_widths(self):
        self.validate(); self.validate(narrow=True)

    def test_runner_registration(self):
        stages, validate = V['C']['case_contract']('reaction-tones')
        self.assertEqual(tuple(stages), ('draft', 'socket', 'refreshed', 'offline'))
        fixture, receipt = sample()
        validate(fixture, receipt, DIR, png=False)

    def test_tone_removed_from_dom_and_codepoints(self):
        self.reject(lambda r:r['geometry']['tones']['pills'][0].update(glyph='\U0001f44d',codepoints=[0x1f44d]))

    def test_paired_tone_order_and_joiner(self):
        for glyph in ('\U0001faf1\U0001f3ff\u200d\U0001faf2\U0001f3fb', '\U0001faf1\U0001f3fb\U0001faf2\U0001f3ff'):
            self.reject(lambda r:r['geometry']['tones']['pills'][2].update(glyph=glyph,codepoints=[ord(c) for c in glyph]))

    def test_claimed_codepoints_without_actual_glyph(self):
        self.reject(lambda r:r['geometry']['tones']['pills'][0].update(glyph=':thumbsup::skin-tone-3:'))

    def test_model_base_glyph_does_not_pass_correct_dom(self):
        self.reject(lambda r:r['model']['toneProbe'][0].update(glyph='\U0001f44d'))

    def test_alias_raw_name_not_canonicalized(self):
        self.reject(lambda r:r['geometry']['tones']['pills'][1].update(name=NAMES[0]))
        self.reject(lambda r:r['model']['comments']['pageMessages'][1]['reactions'][1].update(name=NAMES[0]))

    def test_alias_socket_cannot_increment_sibling(self):
        self.reject(lambda r:r['model']['comments']['pageMessages'][1]['reactions'][0].update(count=2))
        self.reject(lambda r:r['geometry']['tones']['pills'][0].update(count=2))

    def test_original_raw_page_not_rewritten_by_socket(self):
        self.reject(lambda r:r['model']['comments']['page']['messages'][1]['reactions'][1].update(count=2))

    def test_future_raw_source_not_trusted_from_projection(self):
        self.reject(lambda r:r['proof']['serverResponse']['messages'][1].update(tone_fixture_opaque={}),2)

    def test_unknown_name_exact_fallback(self):
        self.reject(lambda r:r['geometry']['tones']['pills'][5].update(glyph=':not_real:',codepoints=[ord(c) for c in ':not_real:']))

    def test_custom_media_cannot_be_remote_or_missing(self):
        for update in (dict(src='https://fixture.invalid/reaction-tone-custom.png'),dict(complete=False),dict(naturalWidth=0),dict(alt=':wave:')):
            self.reject(lambda r:r['geometry']['tones']['pills'][4]['media'].update(update))
        self.reject(lambda r:r['model']['toneMedia'].update(network=True))

    def test_glyph_clipping_and_hit(self):
        self.reject(lambda r:r['geometry']['tones']['pills'][2].update(glyphVisible=False))
        self.reject(lambda r:r['geometry']['tones']['pills'][2].update(glyphHit=False))
        self.reject(lambda r:r['geometry']['tones']['pills'][2]['glyphRect'].update(left=-5))

    def test_action_is_disabled_not_generic_span(self):
        for update in (dict(tag='SPAN'),dict(disabled=False),dict(pressed='true'),dict(count=True)):
            self.reject(lambda r:r['geometry']['tones']['pills'][0].update(update))
        self.reject(lambda r:r['model']['toneProbe'][0].update(available=True))

    def test_no_reaction_or_comment_write(self):
        self.reject(lambda r:r['model']['reactionLedger']['prepared'].append(dict(method='reactions.add')))
        self.reject(lambda r:r['model']['composeLedger']['posts'].append(dict(method='chat.postMessage')))
        self.reject(lambda r:r['model'].update(networkCalls=1))

    def test_exact_parsed_frame_and_source(self):
        self.reject(lambda r:r['proof']['toneEvent'].update(generation=6))
        self.reject(lambda r:r['proof']['toneEvent']['frame'].update(reaction=NAMES[0]))
        self.reject(lambda r:r['proof']['toneEvent'].update(parsed=False))

    def test_no_extra_socket_read(self):
        self.reject(lambda r:r['model']['ledger']['requests'].append(copy.deepcopy(r['model']['ledger']['requests'][0])))
        self.reject(lambda r:r['model']['comments'].update(read=11))

    def test_draft_and_independent_journal_retained(self):
        self.reject(lambda r:r['geometry']['editor'].update(sameNode=False))
        self.reject(lambda r:r['proof']['journal'].update(blocks=[]))
        self.reject(lambda r:r['geometry']['editor'].update(bold=False))
        self.reject(lambda r:r['geometry'].update(active='slack-list-comments-refresh'))

    def test_real_draft_text_line_required(self):
        self.reject(lambda r:r['geometry']['action']['editorText'].update(ok=False,visibleLines=0))

    def test_offline_retains_content_and_notice(self):
        self.reject(lambda r:r['model']['comments']['page']['messages'][1].update(reactions=[]),3)
        self.reject(lambda r:r['geometry']['tones']['notices'][0].update(visible=False),3)


if __name__ == '__main__': unittest.main()
