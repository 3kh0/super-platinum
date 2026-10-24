"""Bounded emoji title evidence over the unchanged Profile file-title lifecycle."""
from pathlib import Path
import json
import runpy
import sys

FLAG = 'SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CHECK'
CASE_FLAG = 'SUPER_PLATINUM_PROFILE_EMOJI_TITLE_CASE'
CASES = ('ordinary', 'empty')
ID = 'FTITLE0001'
DESCRIPTOR = dict(locale='en-US', format='canonical-colon-name',
                  catalog='retained-v16-v17-canonical-intersection', origin='application-presentation')
FALLBACK = dict(locale='en-US', namespace='canvas', source='Untitled', value='Untitled',
                origin='application-presentation')


def same(a, b):
    return json.dumps(a, sort_keys=True, separators=(',', ':')) == json.dumps(b, sort_keys=True, separators=(',', ':'))


class Oracle:
    def __init__(self, case):
        if case not in CASES:
            raise RuntimeError('Unknown exact emoji title case')
        self.case = case
        self.original = 'Project plan' if case == 'ordinary' else ''
        self.renamed = 'Project :wave: plan' if case == 'ordinary' else ':wave: Plan'
        self.base = runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
        self.g = self.base['validate'].__globals__
        self.g['MODE'].update(list=False, blocks=True)
        self.g.update(label=lambda: self.original, renamed=lambda: self.renamed,
                      title_blocks=self.title_blocks, expected_source=self.expected_source)
        self.stages = self.base['STAGES']
        self.fixtures = self.base['FIXTURES']
        self.require = self.base['require']

    def title_blocks(self, changed=False):
        def emoji():
            return dict(type='emoji', name='wave', style=dict(bold=True), opaque=dict(keep=1))
        if self.case == 'ordinary':
            leaves = ([dict(type='text', text='Project '), emoji(), dict(type='text', text=' plan')]
                      if changed else [emoji(), dict(type='text', text=' Project plan')])
        else:
            leaves = [emoji(), emoji(), dict(type='text', text=' Plan')] if changed else [emoji()]
        return [dict(type='rich_text', block_id='kept-block', elements=[dict(type='rich_text_section', elements=leaves)])]

    def raw_file(self, changed=False):
        return dict(ok=True, file=dict(id=ID, mode='quip', filetype='quip', created=101,
                    updated=102 if changed else 101,
                    title='Misleading renamed fallback' if changed else 'Misleading Canvas fallback',
                    name='wrong-name-fallback', fixture_opaque=dict(preserve=[ID, 1, 2]),
                    title_blocks=self.title_blocks(changed)))

    def snapshot(self):
        return dict(case=self.case, original=self.title_blocks(), renamed=self.title_blocks(True),
                    label=self.original, renamedLabel=self.renamed)

    def expected_source(self, index):
        count = (1, 1, 1, 1, 2, 3, 4, 4, 5, 5)[index]
        completed = (0, 1, 1, 1, 2, 3, 3, 4, 5, 5)[index]
        requests = [dict(serial=i+1, method='files.info', args=dict(file=ID, page='1', count='500', truncate='true'))
                    for i in range(count)]
        for row in requests:
            serial = row['serial']
            if serial in (1, 4) and index >= (1 if serial == 1 else 7):
                row['releasedResponse'] = self.raw_file()
            if serial == 1 and index >= 1:
                row['response'] = dict(raw=self.raw_file())
            if serial == 2:
                row['response'] = dict(error='offline')
            if serial in (3, 5):
                row['response'] = dict(raw=self.raw_file())
        return dict(requests=requests, completed=completed, cancelled=int(index >= 7),
                    held=index in (0, 6), active=count-completed, networkCalls=0)

    def validate(self, fixture, receipt, captures, *, platform=sys.platform, png=True):
        self.base['validate'](fixture, receipt, captures, platform=platform, png=png)
        require = self.require
        require(same(receipt.get('emojiTitle'), self.snapshot()), 'Missing exact original emoji title vector')
        for index, row in enumerate(receipt['stages']):
            proof, stage = row['proof'], row['stage']
            require(same(proof.get('emojiTitle'), self.snapshot()), 'Original emoji title source changed')
            require(same(proof.get('source'), self.expected_source(index)), 'Original response/cancellation history changed')
            if index:
                cache = proof['cache']
                changed = stage in ('renamed-mounted', 'confirmed')
                require(same(cache.get('source'), self.raw_file(changed)['file']), 'Full source envelope changed')
                require(same(cache['effective'].get('title_blocks'), self.title_blocks(changed)), 'Emoji leaf/style/opaque source changed')
                current = stage not in ('unavailable', 'held', 'cancelled')
                require(same(cache.get('emojiText'), dict(current=current, descriptor=DESCRIPTOR)),
                        'Missing current explicit English canonical emoji context')
                require(same(cache.get('canvasFallback'), dict(current=current, descriptor=FALLBACK)),
                        'Independent Untitled observation changed')
                require(cache.get('titleUsers') is None, 'Emoji-only title borrowed member authority')
