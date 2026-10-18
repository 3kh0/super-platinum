#!/usr/bin/env python3
"""Existing whole Canvas/list token acceptance; never builds or uses live Slack.

Uses the accepted mentions launcher, native process custody and original PNG
capture protocol. Run --list or --blocks separately in fresh directories.
"""
import argparse
import json
from pathlib import Path
import runpy
import sys

_loaded = runpy.run_path(str(Path(__file__).with_name('check-profile-mentions-native.py')))
B = _loaded['main'].__globals__
require = B['require']
FIXTURES = B['FIXTURES']
STAGES = ('cold-loading','opened','renamed-mounted','confirmed','unavailable','retried','held','cancelled','limit5000','limit5001')
FLAG = 'SUPER_PLATINUM_PROFILE_FILE_TITLES_CHECK'
MODE = {'list':False,'blocks':False}
ID, URL = 'FTITLE0001', 'https://fixture.slack.com/docs/FTITLE0001'
LOADING = 'Loading document titles. Your description is unchanged.'
UNAVAILABLE = 'Document titles are unavailable or changed while opening. Your description is unchanged. Retry to load document details.'
RICH = {'opened','renamed-mounted','confirmed','retried','limit5000'}


def kind(): return 'file' if MODE['list'] else 'canvas'
def label(): return '&amp; *literal*' if MODE['blocks'] else 'List & Plan' if MODE['list'] else 'Canvas & Plan'
def raw_title(changed=False):
    if MODE['blocks']:return 'Misleading renamed fallback' if changed else 'Misleading Canvas fallback'
    return renamed() if changed else '  List &amp; Plan  ' if MODE['list'] else 'Canvas &amp; Plan'
def renamed(): return 'Changed &amp;' if MODE['blocks'] else 'Renamed List' if MODE['list'] else 'Renamed Canvas'
def units(text): return len(text.encode('utf-16-le'))//2


def title_blocks(changed=False):
    values=[':memo: ','Changed ','&amp;'] if changed else [':wave: ','&amp; ','*literal*']
    runs=[dict(type='text',text=value,opaque_run=dict(index=i)) for i,value in enumerate(values)]
    runs[1]['style']=dict(italic=True);runs[2]['style']=dict(bold=True)
    return [dict(type='rich_text',block_id='file-title-block',opaque_block=dict(retain=['block',7]),elements=[
        dict(type='rich_text_section',opaque_section=dict(retain=True),elements=runs)])]


def prefix(stage):
    if stage in STAGES[:3]: return 'Working with '
    if stage.startswith('limit'): return 'x'*(int(stage[5:])-2*units(label()))
    return 'Saved with '


def blocks(text):
    token=lambda i,style:dict(type=kind(),file_id=ID,text=URL,url=URL,fixture_occurrence=i,style=style,opaque=dict(keep=[i,'original']))
    return [dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[
        dict(type='text',text=text),token(1,dict(bold=True)),token(2,dict(italic=True))])])]


def expected_source(index):
    count=(1,1,1,1,2,3,4,4,5,5)[index]
    completed=(0,1,1,1,2,3,3,4,5,5)[index]
    requests=[dict(serial=i+1,method='files.info',args=dict(file=ID,page='1',count='500',truncate='true')) for i in range(count)]
    return dict(requests=requests,completed=completed,cancelled=int(index>=7),held=index in (0,6),active=count-completed,networkCalls=0)


def validate_geometry(g,viewport,stage):
    require(isinstance(g,dict) and g.get('ok') is True and g.get('noOverflow') is True and g.get('viewport')==viewport,
            'Missing layout or viewport proof')
    require(g.get('dialog') is True,'Profile dialog disappeared')
    B['rectangle'](g.get('dialogRect'),viewport)
    for key in ('controls','notices','tokens'):
        require(isinstance(g.get(key),list),'Missing UI inventory')
        for item in g[key]:
            require(item.get('visible') is True,'Control, feedback or token clipped/hidden')
            B['rectangle'](item.get('rect'),viewport)
    rich=stage in RICH
    unavailable=stage in ('cold-loading','unavailable','held')
    controls=['Back','Save section']
    if rich: controls+=['Use description','Cancel description edit']
    if unavailable: controls+=['Retry description','Cancel description edit']
    require(sorted(c.get('text') for c in g['controls'])==sorted(controls),'Wrong visible actions')
    for c in g['controls']:
        disabled=c['text']=='Save section' or (c['text']=='Retry description' and stage in ('cold-loading','held'))
        require(c.get('disabled') is disabled,'Incorrect native action lock')
    wanted={ 'cold-loading':LOADING,'held':LOADING,'unavailable':UNAVAILABLE,
             'confirmed':'Profile fields saved.','limit5001':"Check this field's value or length." }.get(stage)
    if wanted: require(wanted in [n.get('text') for n in g['notices']],'Missing exact recovery feedback')
    require(g.get('richEditable') is (True if rich else None),'Wrong editor state')
    mounted=renamed() if stage=='confirmed' else label()
    require(len(g['tokens'])==(2 if rich else 0),'Wrong duplicate token count')
    for token in g['tokens']:
        require(token.get('text')==mounted and token.get('editable') is False,'Incorrect canonical frozen label')
    if rich:
        require([(token.get('fontWeight'),token.get('fontStyle')) for token in g['tokens']]
                == [('700','normal'),('400','italic')], 'Actual computed file token formatting lost bold/italic')
    captured=g.get('captured')
    if rich:
        require(isinstance(captured,dict),'Missing native editor receipt')
        snap=captured.get('snapshot',{})
        require(snap.get('blocks')==blocks(prefix(stage)),'Native file token/URL/opaque/style drift')
        require(snap.get('composing') is False and type(snap.get('revision')) is int and snap['revision']>=0
                and isinstance(snap.get('generation'),str) and snap['generation'].isdigit(),'Invalid native custody')
        require(captured.get('tokens')==[dict(value=t,label=mounted) for t in blocks('')[0]['elements'][0]['elements'][1:]],
                'Ordered native file witnesses changed')
    else: require(captured is None,'Hidden editor capture')
    if stage=='limit5001':
        require(g.get('longDisabled') is False and g.get('managedFieldPreserved') is True,'Invalid draft cannot be corrected')
        require(g.get('choiceValue')=='Morning' and g.get('countryValue')=='US','Native siblings changed')
    dims=g.get('pngDimensions')
    require(isinstance(dims,list) and len(dims)==2 and all(type(x) is int and x>0 for x in dims),'Invalid PNG dimensions')


def validate(fixture,receipt,captures,*,platform=sys.platform,png=True):
    require(fixture in FIXTURES and isinstance(receipt,dict) and receipt.get('fixture')==fixture,'Wrong receipt owner')
    require(receipt.get('ok') is True,f"Native file assertion failed: {receipt.get('error')}")
    require(receipt.get('mentionKind')==kind(),'Wrong file kind')
    require(receipt.get('titleBlocks',False) is MODE['blocks'],'Wrong Canvas title source mode')
    require(receipt.get('captureChecksPassed') is True and receipt.get('visualReview')=='Original PNG inspection required separately',
            'Missing independent visual review boundary')
    viewport=B['expected_viewport'](fixture,platform)
    require(receipt.get('viewport')==viewport,'Wrong viewport')
    rows,paths=receipt.get('stages'),receipt.get('captures')
    require(isinstance(rows,list) and [r.get('stage') for r in rows]==list(STAGES),'Missing/reordered file stages')
    require(isinstance(paths,dict) and set(paths)==set(STAGES),'Wrong PNG inventory')
    previous=None
    for index,row in enumerate(rows):
        stage,model,proof,g=row['stage'],row.get('model'),row.get('proof'),row.get('geometry')
        require(isinstance(model,dict) and isinstance(proof,dict),'Missing actual model/source')
        require(model.get('enabled') is True and model.get('account')=='00000000-0000-4000-8000-000000000081'
                and type(model.get('epoch')) is int,'Wrong fixture identity')
        require(type(model.get('visit')) is int and type(model.get('serial')) is int,'Missing editor owner')
        if previous:
            require(model['epoch']==previous['epoch'] and model['visit']==previous['visit'],'Unexpected editor ownership change')
            require((model['serial']!=previous['serial']) if stage=='confirmed' else model['serial']==previous['serial'],
                    'Confirmed readback did not exclusively replace section serial')
        require(model.get('blocks')==blocks(prefix(stage)),'Draft lost original file tokens/URL/opaque/styles')
        require(model.get('busy') is False and model.get('uncertain') is False,'Wrong section write state')
        require(model.get('managedValue')=='Preserved organization value' and model.get('choiceValue')=='Morning'
                and model.get('countryValue')=='US','Untouched sibling changed')
        require(model.get('canSave') is (stage=='limit5000'),'Wrong section Save availability')
        require(model.get('reads')==(1 if index>=3 else 0),'Unexpected profile readback')
        attempts=model.get('attempts')
        require(isinstance(attempts,list) and len(attempts)==(1 if index>=3 else 0),'Unexpected section write/replay')
        if attempts:
            values={v.get('element_id'):v for v in attempts[0]}
            require(len(values)==len(attempts[0]) and {'long','locked'}<=set(values),'Wrong write inventory')
            require(json.loads(values['long']['long_text']['blocks'])==blocks('Saved with '),'Wire substituted display title or lost custody')
            require(values['locked'].get('text',{}).get('text')=='Preserved organization value','Managed wire value changed')
            if previous and previous['attempts']:require(attempts==previous['attempts'],'Saved wire changed')
        require(proof.get('source')==expected_source(index),'Closed files.info request/settlement changed')
        cache=proof.get('cache')
        if stage=='cold-loading': require(cache is None,'Cold file cache was preseeded')
        else:
            require(isinstance(cache,dict) and cache.get('current') is (stage not in ('unavailable','held','cancelled')),'Wrong title cache readiness')
            changed=stage in ('renamed-mounted','confirmed')
            title=raw_title(changed)
            for part in ('source','effective'):
                value=cache.get(part,{})
                require(value.get('id')==ID and value.get('title')==title and value.get('mode')==('list' if MODE['list'] else 'quip'),
                        'Actual raw/effective title history differs')
                if MODE['blocks']:
                    require(value.get('title_blocks')==title_blocks(changed),'Original split block title/style/opaque history changed')
            if MODE['blocks']:
                require(cache.get('quipTitle')==dict(current=True,value='nullish',fileId=ID),
                        'Missing current real runtime Quip title absence observation')
            require(cache['source'].get('fixture_opaque')==dict(preserve=[ID,1,2]),'Source opaque fields lost')
            require('fixture_opaque' not in cache['effective'],'Title facts claimed unrelated full file projection')
        exercise=proof.get('exercise',{})
        expected={ 'cold-loading':dict(cold=True),'opened':dict(release=dict(delivered=True,cancelledBeforeReply=False)),
          'renamed-mounted':dict(frozenLabel=label()),'confirmed':dict(reopenedLabel=renamed()),
          'unavailable':dict(draftRetained=True),'retried':dict(draftRetained=True),'held':dict(originalOpening=True),
          'cancelled':dict(release=dict(delivered=False,cancelledBeforeReply=True)),
          'limit5000':dict(sourceLength=5000,labelUnits=units(label()),occurrences=2),
          'limit5001':dict(sourceLength=5001,labelUnits=units(label()),occurrences=2)}[stage]
        require(exercise==expected,'Missing or incorrect behavioral proof')
        validate_geometry(g,viewport,stage)
        path=Path(paths[stage]) if isinstance(paths[stage],str) else None
        require(path is not None and path.is_absolute() and path==captures.resolve()/f'{fixture}-{stage}.png','Wrong owned PNG path')
        if png:
            require(path.is_file() and not path.is_symlink(),'Missing original PNG')
            B['validate_png'](path,g['pngDimensions'])
        previous=model


_native=B['NativeFixture']
class FileFixture(_native):
    def __init__(self,fixture,log,binary,extra):
        extra=dict(extra,**{FLAG:'1','SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND':'list' if MODE['list'] else 'canvas',
                           'SUPER_PLATINUM_PROFILE_FILE_TITLE_BLOCKS':'1' if MODE['blocks'] else '0'})
        super().__init__(fixture,log,binary,extra)

B.update(PREFIX='profile-file-titles-check: ',stages=lambda:STAGES,validate=validate,kind=kind,NativeFixture=FileFixture)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=B['DEFAULT_BINARY'])
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=FIXTURES)
    mode=parser.add_mutually_exclusive_group()
    mode.add_argument('--list',action='store_true',help='Whole-list file tokens; default is Canvas')
    mode.add_argument('--blocks',action='store_true',help='Canvas text-only rich title blocks with actual runtime absence')
    args=parser.parse_args(); MODE.update(list=args.list,blocks=args.blocks)
    for fixture in args.fixture or FIXTURES:
        require(not (args.captures/f'{fixture}.log').exists(),'Use fresh capture directory')
        B['run_fixture'](fixture,Path(args.binary).resolve(),args.captures.resolve())


if __name__=='__main__': main()
