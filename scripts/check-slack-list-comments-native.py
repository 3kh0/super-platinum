#!/usr/bin/env python3
"""Owned offline List comment reads. Never builds or connects to Slack."""
import argparse
from contextlib import ExitStack
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import runpy
import secrets
import sys
import time
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture

H = runpy.run_path(str(Path(__file__).with_name('check-slack-list-hydration-native.py')))
FLAG = 'SUPER_PLATINUM_LIST_COMMENTS_CHECK'
PREFIX, WAITING, EDGE = ('slack-list-comments-' + s + ': ' for s in ('check', 'waiting', 'stage'))
FIXTURES = ('slack-list', 'slack-list-narrow')
STAGES = ('detail','loading','first','next','malformed','wrong-root','offline','empty','cooldown','back','synthetic','record-held','record-replaced','account-held','account-replaced')
COUNTS = (0,4,4,8,12,16,20,24,28,28,32,36,36,40,40)
LIST, KNOWN, ALIAS, ROOT_TS, SYNTHETIC_TS = 'FCOMMENT01','R_LIST_000','R_LIST_001','1781524800.000001','1781524900.000001'
CURSOR = 'comments-next+/='
GENERIC = 'Comments could not be loaded. Previously loaded comments are retained. Refresh to try again.'
OFFLINE = 'You are offline. Previously loaded comments are retained.'
COOLDOWN = 'Slack asked us to wait. Previously loaded comments are retained.'


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def record_proof(records):
    """Non-security checksum of sorted UTF-8 JSON, paired with exact touched rows."""
    require(isinstance(records,dict), 'Missing original record map')
    touched={k:v for k,v in records.items() if k in (KNOWN,ALIAS)}
    other={k:v for k,v in records.items() if k not in (KNOWN,ALIAS)}
    data=json.dumps(other,ensure_ascii=False,sort_keys=True,separators=(',',':'),allow_nan=False).encode('utf-8')
    value=0xcbf29ce484222325
    for byte in data:value=((value^byte)*0x100000001b3)&0xffffffffffffffff
    return touched,dict(count=len(other),byteLength=len(data),fnv1a64=f'{value:016x}')


def ready_tuple(fixture, nonce, pid):
    require(fixture in FIXTURES and isinstance(nonce,str) and re.fullmatch('[0-9a-f]{32}',nonce)
            and type(pid) is int and pid>0, 'Invalid comments readiness owner')
    return dict(version=1,nonce=nonce,pid=pid,fixture=fixture)


def rectangle(rect, viewport):
    require(isinstance(rect,dict) and all(type(rect.get(k)) in (int,float) and math.isfinite(rect[k])
            for k in ('left','top','right','bottom','width','height')), 'Missing finite geometry')
    require(rect['width']>0 and rect['height']>0 and rect['left']>=0 and rect['top']>=0
            and rect['right']<=viewport[0]+1 and rect['bottom']<=viewport[1]+1
            and abs(rect['right']-rect['left']-rect['width'])<=1
            and abs(rect['bottom']-rect['top']-rect['height'])<=1, 'Comments control clipped')


def requests(rows, platform):
    require(isinstance(rows,list), 'Missing comments request ledger')
    sequence=('files.info','lists.records.info','experiments.getByUser','conversations.replies')
    for i,row in enumerate(rows):
        method=sequence[i%4]; operation=i//4
        require(row.get('serial')==i+1 and row.get('method')==method, 'Wrong request sequence/serial')
        record=KNOWN if operation<7 else ALIAS
        args=({'file':LIST} if i%4==0 else
              {'list_id':LIST,'id':record,'include_is_subscribed':'true','_x_reason':'get-record-subtasks'} if i%4==1 else
              {'ssb_platform':{'win32':'windows','darwin':'darwin'}.get(platform,platform),'ssb_release_channel':'prod'} if i%4==2 else
              {'channel':'CCOMMENT01','ts':ROOT_TS if operation<7 else LIST+'-'+ALIAS,'inclusive':'true','limit':'28',
               'cached_latest_updates':'{}','_x_reason':'history-api/fetchReplies','include_tombstones':'true'})
        if i%4==3 and operation==1: args['cursor']=CURSOR
        require(row.get('args')==args, 'Unexpected read endpoint/options/root/cursor or secret field')


def validate(fixture, receipt, directory, *, platform=sys.platform, png=True, dimensions=None):
    require(fixture in FIXTURES and receipt.get('ok') is True, 'Native comments failed: '+str(receipt.get('error')))
    require(receipt.get('fixture')==fixture and receipt.get('noLiveNetwork') is True, 'Wrong comments fixture/isolation')
    stages=receipt.get('stages')
    require(isinstance(stages,list) and [s.get('stage') for s in stages]==list(STAGES), 'Missing comments stages')
    initial=stages[0]['model']; old_requests=[]; original_viewport=None; previous=None
    original=receipt.get('initialRecords'); touched,unrelated=record_proof(original)
    require(len(original)==73 and set(touched)=={KNOWN,ALIAS} and unrelated['count']==71, 'Incomplete original record inventory')
    require(initial['initialDocument']['id']==LIST and initial['initialRecords'][KNOWN].get('thread_ts')==ROOT_TS
            and not initial['initialRecords'][ALIAS].get('thread_ts'), 'Known/synthetic roots not initially observed')
    pages={}
    for i,row in enumerate(stages):
        stage=row['stage']; model=row.get('model',{}); g=row.get('geometry',{}); proof=row.get('proof') or {}
        ledger=model.get('ledger',{}); entry=model.get('comments'); active=stage not in ('detail','back','record-replaced','account-replaced')
        held=stage in ('loading','record-held','account-held'); cancelled=0 if i<12 else 1 if i<14 else 2
        require(model.get('noSession') is True and model.get('noTransport') is True
                and model.get('writes')==model.get('networkCalls')==0, 'Comments introduced live IO or writes')
        requests(ledger.get('requests'), platform)
        require(len(ledger['requests'])==COUNTS[i] and ledger['requests'][:len(old_requests)]==old_requests,
                'Read replay or changed history')
        old_requests=ledger['requests']
        require(ledger.get('replies')==COUNTS[i]//4 and ledger.get('completed')==COUNTS[i]-int(held)
                and ledger.get('held') is held and ledger.get('cancelled')==cancelled, 'Incorrect completion/cancellation ledger')
        require(model.get('initialDocument')==initial['initialDocument'] and model.get('initialRecords')==touched and model.get('initialUnrelated')==unrelated,
                'Immutable original source changed')
        if stage!='account-replaced':
            require(model.get('account')==initial.get('account') and model.get('epoch')==initial.get('epoch')
                    and model.get('document')==initial['initialDocument'], 'Original owner/document changed')
            expected=copy.deepcopy(initial['initialRecords'])
            if i>=12:
                estimate=[f for f in expected[ALIAS]['fields'] if f['key']=='estimate']
                require(len(estimate)==1,'Missing original number field');estimate[0]['number']=[19]
            require(model.get('records')==expected and model.get('unrelated')==unrelated, 'Unexpected canonical record mutation')
            require(model.get('detail',{}).get('record')==(KNOWN if i<10 else ALIAS), 'Wrong open record')
        else:
            require(model.get('account')=='list-comments-successor' and model.get('epoch')==initial['epoch']+1
                    and model.get('document') is None and model.get('records') is None and model.get('detail') is None
                    and model.get('unrelated') is None and entry is None, 'Old account comments survived replacement')
        require((isinstance(entry,dict) and entry.get('active') is True)==active, 'Wrong comment owner visibility')
        if active:
            require(entry.get('loading') is held and entry.get('record')==(KNOWN if i<10 else ALIAS), 'Wrong read/record custody')
            if stage not in ('loading','account-held'):
                require(entry.get('root')==(ROOT_TS if i<10 else SYNTHETIC_TS), 'Acquired root changed')
        page=entry.get('page') if isinstance(entry,dict) else None
        pages[stage]=page
        if stage in ('first','synthetic'):
            raw_rows=(page or {}).get('messages')
            root_ts=ROOT_TS if stage=='first' else SYNTHETIC_TS
            record=KNOWN if stage=='first' else ALIAS
            require(isinstance(raw_rows,list) and len(raw_rows)==2
                    and raw_rows[0].get('ts')==root_ts and raw_rows[0].get('subtype')=='list_record_comment'
                    and raw_rows[0].get('slack_list')==dict(list_id=LIST,list_record_id=record)
                    and raw_rows[0].get('unknown')==dict(keep=True), 'Synthetic/known acquired root source mismatch')
        if active:
            expected_rows=[] if stage in ('loading','empty','cooldown','account-held') else ['1781524801.000001'] if stage=='first' else ['1781524802.000001'] if stage in ('next','malformed','wrong-root','offline') else ['1781524901.000001']
            require(entry.get('visibleTs')==expected_rows, 'Root stub leaked or comment projection changed')
        if stage=='first':
            require(proof.get('release')==dict(delivered=True,cancelledBeforeReply=False), 'Original held read did not complete')
            require(page.get('has_more') is True and page.get('response_metadata',{}).get('next_cursor')==CURSOR, 'First cursor lost')
        if stage in ('malformed','wrong-root','offline'):
            require(page==pages['next'], 'Failure erased/replaced previously loaded page')
        if stage=='empty': require(page==dict(ok=True,messages=[],has_more=False), 'Empty page not accepted')
        if stage=='cooldown':
            require(page==pages['empty'] and entry.get('retryAfter',0)>0 and proof.get('native')==dict(ok=True,disabled=True)
                    and proof.get('before')==proof.get('after')==ledger, 'Cooldown allowed another request')
        error=GENERIC if stage in ('malformed','wrong-root') else OFFLINE if stage=='offline' else COOLDOWN if stage in ('cooldown','back') else None
        if active: require(entry.get('error')==error, 'Wrong failure/refresh state')
        if stage in ('record-replaced','account-replaced'):
            require(proof.get('before')==previous and proof.get('release')==dict(delivered=False,cancelledBeforeReply=True),
                    'Retired original request delivered or lacks exact predecessor')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('editable')==0
                and g.get('comments') is active and g.get('dialog') is (stage!='account-replaced'), 'Wrong read-only layout')
        viewport=g.get('viewport'); require(isinstance(viewport,list) and len(viewport)==2
                and all(type(n) in (int,float) and n>0 for n in viewport), 'Missing viewport')
        if original_viewport is None: original_viewport=viewport
        require(viewport==original_viewport, 'Viewport changed')
        if active: require(g.get('generation')==str(entry['generation']), 'Native controller generation differs')
        controls=g.get('controls',[]); expected_ids=({'slack-list-comments-back','slack-list-comments-refresh'} if active else
                {'slack-list-record-back','slack-list-record-comments'} if stage!='account-replaced' else set())
        if active and entry.get('hasMore'): expected_ids.add('slack-list-comments-next')
        require({c.get('id') for c in controls}==expected_ids and len(controls)==len(expected_ids), 'Missing/duplicate controls')
        for c in controls:
            require(c.get('visible') is True and c.get('hit') is True, 'Control obscured')
            rectangle(c.get('rect'),viewport)
            if c['id']=='slack-list-comments-back':
                require(c.get('display')=='flex' and c.get('fits') is True and c.get('iconCount')==1, 'Back icon/text layout overflow')
            if active and c['id']!='slack-list-comments-back': require(c.get('disabled') is (held or stage=='cooldown'), 'Wrong read/cooldown disabled state')
        for item in g.get('notices',[])+g.get('rows',[]):
            require(item.get('visible') is True,'Comment/notice clipped');rectangle(item.get('rect'),viewport)
        require([r.get('ts') for r in g.get('rows',[])]==(entry.get('visibleTs') if active else []), 'Native rows not exact projection')
        for native_row in g.get('rows',[]):
            expected_text={'1781524801.000001':'First page comment.','1781524802.000001':'Second page retained.','1781524901.000001':'Synthetic root resolved.'}.get(native_row.get('ts'))
            require(expected_text is not None and expected_text in native_row.get('text',''),'Canonical comment text not rendered')
        texts='\n'.join(n.get('text','') for n in g.get('notices',[]))
        if stage=='first':
            content=g.get('readContent',{})
            # This read fixture admits no reaction dispatcher. Compare the
            # actual backend admission proof to its intentionally disabled UI.
            require(proof.get('reactionAvailable') is False,'Closed read fixture admitted a reaction')
            require(content.get('bold') is True and content.get('secondParagraph') is True
                    and content.get('reactionCount')=='2' and content.get('reactionGlyph')=='\U0001f44b'
                    and content.get('reactionTag')=='BUTTON' and content.get('reactionType')=='button'
                    and content.get('reactionVisible') is True and content.get('reactionHit') is True
                    and content.get('reactionDisabled') is True and content.get('reactionAriaDisabled')=='true'
                    and content.get('reactionOwn') is False and content.get('reactionPressed')=='false'
                    and type(content.get('messageTools')) is int and content['messageTools']==0
                    and type(content.get('reactionButtons')) is int and content['reactionButtons']==1,
                    'Rich comment or scoped List reaction button proof missing')
        if error and active: require(error in texts,'Failure notice absent')
        if held: require('Loading comments' in texts,'Loading feedback absent')
        if stage=='empty': require('No comments yet' in texts,'Empty feedback absent')
        if stage=='back': require(g.get('active')=='slack-list-record-comments','Back did not restore Comments focus')
        path=directory/f'{fixture}-{stage}.png';require(Path(row.get('capture',''))==path,'Foreign capture path')
        if png: H['png_signature'](path,dimensions)
        previous=model
    # Retry countdown may tick after the last capture; terminal has no active entry.
    require(receipt.get('final')==stages[-1]['model'],'Terminal state changed')
    return receipt


def validate_log(native, fixture, directory, expected, dimensions):
    waiting, receipts = native.receipts(WAITING), native.receipts(PREFIX)
    require(len(waiting)==len(receipts)==1,'Missing or duplicate ownership/terminal receipt')
    H['validate_ready'](waiting[0],expected)
    # Early native failure receipts intentionally have only ok/error. Preserve
    # that failure instead of masking it with success-only ownership fields.
    require(receipts[0].get('ok') is True, 'Native comments failed: '+str(receipts[0].get('error')))
    H['validate_ready'](receipts[0].get('readiness'),expected)
    require(native.receipts(EDGE)==[dict(stage=s,edge=e) for s in STAGES for e in ('before','after')], 'Capture edges changed')
    return validate(fixture,receipts[0],directory,dimensions=dimensions)


def run_fixture(fixture, binary, captures):
    require(fixture in FIXTURES, 'Unknown List comments fixture')
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'slack-list-comments-{nonce}'
    directory.mkdir(exist_ok=False)
    (directory/'owner.json').write_text(json.dumps(dict(version=1,nonce=nonce,fixture=fixture)),encoding='utf-8')
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG:'1','SUPER_PLATINUM_LIST_COMMENTS_NONCE':nonce,
                            'SUPER_PLATINUM_LIST_COMMENTS_READY_DIR':str(directory),
                            'SUPER_PLATINUM_LIST_COMMENTS_CAPTURE_DIR':str(directory)})
    process_api = handle = None
    try:
        with native:
            evidence['pid'] = native.process.pid
            if sys.platform == 'win32':
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence['processCreationFiletime'] = process_api.times(handle)['creation_filetime']
            try:
                with ExitStack() as stack:
                    expected = ready_tuple(fixture, nonce, native.process.pid)
                    deadline = time.monotonic() + 25
                    while not (waiting := native.receipts(WAITING)):
                        native.require_running()
                        require(not native.receipts(PREFIX) and not native.receipts(EDGE)
                                and time.monotonic() < deadline, 'Readiness failed or timed out')
                        time.sleep(.05)
                    require(len(waiting) == 1 and not native.receipts(EDGE) and not native.receipts(PREFIX),
                            'Duplicate readiness or interaction before placement')
                    H['validate_ready'](waiting[0], expected)
                    pointer = H['normalize'](native, evidence, stack, process_api, handle)
                    H['publish_ready'](directory, expected)
                    evidence['readiness'] = expected
                    deadline, sampled, count = time.monotonic() + 180, 0., 0
                    while True:
                        native.require_running()
                        edges = native.receipts(EDGE)
                        if pointer:
                            for edge in edges[count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check('running', require_clear=True)
                                sampled = time.monotonic()
                        count = len(edges)
                        if native.receipts(PREFIX):
                            evidence['native'] = validate_log(native, fixture, directory, expected, evidence.get('expectedPngDimensions'))
                            if pointer:
                                pointer.check('terminal', require_clear=True)
                            break
                        require(time.monotonic() < deadline, 'List comments receipt timed out')
                        time.sleep(.05)
            except BaseException:
                try:
                    # Keep the named "failure" stage intact if outer receipt
                    # validation fails after the native scenario completes.
                    path = captures / f'{fixture}-harness-failure.png'
                    native.capture(path)
                    evidence['failureScreenshot'] = str(path)
                except (OSError, RuntimeError, ValueError) as error:
                    evidence['failureCaptureError'] = str(error)
                raise
        require(native.process.poll() is not None and not native.cleanup_errors, 'Fixture process or cleanup did not finish cleanly')
        if sys.platform == 'win32':
            H['validate_pointer_cleanup'](evidence.get('pointer', {}))
        require(validate_log(native, fixture, directory, expected, evidence.get('expectedPngDimensions')) == evidence['native'], 'Closed log changed')
        if handle:
            evidence['exitIdentity'] = process_api.times(handle)
            H['validate_exit'](evidence['processCreationFiletime'], evidence['exitIdentity'])
        require(H['binary_sha'](binary) == digest, 'Binary changed during native run')
        evidence.update(ok=True, closedLogRevalidated=True)
    except BaseException as error:
        evidence['error'] = str(error)
        evidence['terminalReceipts'] = native.receipts(PREFIX)
        raise
    finally:
        evidence['processExited'] = native.process is not None and native.process.poll() is not None
        evidence['cleanupErrors'] = native.cleanup_errors
        if handle:
            try:
                evidence['cleanupProcessIdentity'] = process_api.times(handle)
            except OSError as error:
                evidence.update(ok=False, cleanupIdentityError=str(error))
            process_api.kernel.CloseHandle(handle)
        path = captures / f'{fixture}.json'
        path.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(dict(receipt=str(path), **evidence)), flush=True)
    require(evidence['ok'], 'Final process identity audit failed')



def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path(os.environ.get('SUPER_PLATINUM_BINARY',DEFAULT_BINARY)))
    parser.add_argument('--captures',type=Path,default=ROOT/'tmp/list-comments-native')
    parser.add_argument('--fixture',action='append',choices=FIXTURES)
    args=parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        captures=args.captures.resolve()/fixture
        require(not captures.exists(),'Use a fresh capture directory')
        run_fixture(fixture,args.binary.resolve(),captures)

if __name__=='__main__':
    main()
