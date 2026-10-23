"""Exact link-leaf source checks layered over the unchanged file-title lifecycle."""
import json
from pathlib import Path
import runpy
import sys

FLAG = 'SUPER_PLATINUM_PROFILE_LINK_TITLE_CHECK'
ID = 'FTITLE0001'
ORIGINAL = '&amp; *x* A%2F E N [ ]'
RENAMED = 'Changed &amp; A%2F E N [ ]'
DESCRIPTOR = dict(locale='en-US',namespace='canvas',source='Untitled',value='Untitled',origin='application-presentation')


def title_blocks(changed=False):
    return [dict(type='rich_text',block_id='link-title-block',opaque_block=dict(retain=['link',7]),elements=[
        dict(type='rich_text_section',opaque_section=dict(retain=True),elements=[
            dict(type='link',text='Changed &amp;' if changed else '&amp; *x*',url='https://fixture.invalid/ignored',truncated=True,style=dict(bold=True),opaque=dict(index=0)),
            dict(type='text',text=' '),
            dict(type='link',url='A%2F',style=dict(italic=True),opaque=dict(index=1)),
            dict(type='text',text=' '),
            dict(type='link',text='',url='E',opaque=dict(index=2)),
            dict(type='text',text=' '),
            dict(type='link',text=None,url='N',opaque=dict(index=3)),
            dict(type='text',text=' ['),
            dict(type='link',text=' ',url='SHOULD_NOT_RENDER',opaque=dict(index=4)),
            dict(type='text',text=']'),
            dict(type='link',url=None,opaque=dict(index=5)),
            dict(type='link',text=None,url='',opaque=dict(index=6)),
            dict(type='link',opaque=dict(index=7))])])]


def raw_file(changed=False):
    return dict(ok=True,file=dict(id=ID,mode='quip',filetype='quip',created=101,updated=102 if changed else 101,
        title='Misleading renamed fallback' if changed else 'Misleading Canvas fallback',name='wrong-name-fallback',
        fixture_opaque=dict(preserve=[ID,1,2]),title_blocks=title_blocks(changed)))


def snapshot():
    return dict(original=title_blocks(),renamed=title_blocks(True),label=ORIGINAL,renamedLabel=RENAMED)


def expected_source(index):
    count=(1,1,1,1,2,3,4,4,5,5)[index]
    completed=(0,1,1,1,2,3,3,4,5,5)[index]
    requests=[dict(serial=i+1,method='files.info',args=dict(file=ID,page='1',count='500',truncate='true')) for i in range(count)]
    for row in requests:
        serial=row['serial']
        if serial in (1,4) and index >= (1 if serial==1 else 7):row['releasedResponse']=raw_file()
        if serial==1 and index>=1:row['response']=dict(raw=raw_file())
        if serial==2:row['response']=dict(error='offline')
        if serial in (3,5):row['response']=dict(raw=raw_file())
    return dict(requests=requests,completed=completed,cancelled=int(index>=7),held=index in (0,6),active=count-completed,networkCalls=0)


def same(a,b):
    return json.dumps(a,sort_keys=True,separators=(',',':')) == json.dumps(b,sort_keys=True,separators=(',',':'))


# An isolated runpy namespace changes only expected source/labels; every original
# owner, wire, geometry, length, failure, Retry and cancellation assertion runs.
BASE=runpy.run_path(str(Path(__file__).with_name('check-profile-file-titles-native.py')))
G=BASE['validate'].__globals__
G['MODE'].update(list=False,blocks=True)
G.update(label=lambda:ORIGINAL,renamed=lambda:RENAMED,title_blocks=title_blocks,expected_source=expected_source)
STAGES=BASE['STAGES']
FIXTURES=BASE['FIXTURES']
require=BASE['require']


def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    BASE['validate'](fixture,receipt,captures,platform=platform,png=png)
    require(same(receipt.get('linkTitle'),snapshot()),'Original link-title vector missing or changed')
    for index,row in enumerate(receipt['stages']):
        proof=row['proof'];stage=row['stage']
        require(same(proof.get('linkTitle'),snapshot()),'Original exact link leaves changed')
        require(same(proof.get('source'),expected_source(index)),'Original response/released bytes, factory or completion changed')
        if index:
            cache=proof['cache'];changed=stage in ('renamed-mounted','confirmed')
            require(same(cache.get('source'),raw_file(changed)['file']),'Original full source envelope changed')
            require(same(cache['effective'].get('title_blocks'),title_blocks(changed)),'Effective source leaf types/opaque bytes changed')
            require(same(cache.get('canvasFallback'),dict(current=stage not in ('unavailable','held','cancelled'),descriptor=DESCRIPTOR)),
                    'Original Profile presentation observation changed')
            require(cache.get('titleUsers') is None,'Link-only title borrowed nested member authority')

