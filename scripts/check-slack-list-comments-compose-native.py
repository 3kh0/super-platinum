#!/usr/bin/env python3
"""Owned offline List comment composition; never builds or connects to Slack."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import secrets
import sys
import time
from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture
H=runpy.run_path(str(Path(__file__).with_name('check-slack-list-hydration-native.py')))
READ=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-native.py')))
FLAG='SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CHECK'
PREFIX,WAITING,EDGE=('slack-list-comments-compose-'+s+': ' for s in ('check','waiting','stage'))
FIXTURES=('slack-list','slack-list-narrow')
require=READ['require']
ready_tuple=READ['ready_tuple']
rectangle=READ['rectangle']
STAGES=('draft','back','back-reopen','closed','close-reopen','escape-reopen','scrim-reopen','sending','sent','not-sent','rejected','unconfirmed','locked-reopen','confirmed','denied','record-held','record-replaced','account-held','account-replaced')


def outgoing(row):
    require(isinstance(row,dict) and row.get('method')=='chat.postMessage','Wrong reply endpoint')
    a=row.get('args',{})
    require(set(a)=={'channel','ts','type','reply_broadcast','thread_ts','include_channel_perm_error','client_msg_id','blocks','_x_reason'},'Reply field set changed')
    require(a['channel']=='CCOMMENT01' and a['thread_ts']==READ['ROOT_TS'] and a['type']=='message'
            and a['reply_broadcast']=='false' and a['include_channel_perm_error']=='true'
            and a['_x_reason']=='webapp_message_send','Wrong reply destination/options')
    require(re.fullmatch('[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}',a['client_msg_id']) is not None,'Reply UUID not retained v4')
    require(re.fullmatch(r'\d+\.\d{6}',a['ts']) is not None,'Missing captured client timestamp')
    blocks=json.loads(a['blocks'])
    require(isinstance(blocks,list) and blocks and all(b.get('type')=='rich_text' for b in blocks),'Non-rich outgoing content')
    return blocks


def validate_reads(rows):
    require(isinstance(rows,list),'Missing read ledger')
    for index,row in enumerate(rows):
        a=row.get('args',{});method=row.get('method')
        require(row.get('serial')==index+1,'Read serial changed')
        if method=='files.info':require(a=={'file':READ['LIST']},'Foreign file read')
        elif method=='lists.records.info':
            require(a.get('id') in (READ['KNOWN'],READ['ALIAS']) and a==dict(list_id=READ['LIST'],id=a['id'],include_is_subscribed='true',_x_reason='get-record-subtasks'),'Foreign record read')
        elif method=='experiments.getByUser':
            require(set(a)=={'ssb_platform','ssb_release_channel'} and a['ssb_release_channel']=='prod' and a['ssb_platform'] in ('windows','darwin','linux'),'Experiment read drift')
        elif method=='conversations.replies':
            require(set(a)=={'channel','ts','inclusive','limit','cached_latest_updates','_x_reason','include_tombstones'}
                    and a['channel']=='CCOMMENT01' and a['ts'] in (READ['ROOT_TS'],READ['SYNTHETIC_TS'],READ['LIST']+'-'+READ['ALIAS'])
                    and a['inclusive']=='true' and a['limit']=='28' and a['cached_latest_updates']=='{}'
                    and a['_x_reason']=='history-api/fetchReplies' and a['include_tombstones']=='true','Reply read drift')
        else:raise RuntimeError('Unadmitted read endpoint')


def validate(fixture,receipt,directory,*,dimensions=None,png=True):
    math=__import__('math')
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture
            and receipt.get('noLiveNetwork') is True,'Wrong compose receipt')
    rows=receipt.get('stages',[])
    require([r.get('stage') for r in rows]==list(STAGES),'Missing compose stages')
    original=rows[0]['geometry']['editor']['blocks']
    require(original==rows[0]['proof']['blocks'],'Original native capture changed')
    seed=rows[0]['proof'].get('seed',{})
    require(seed.get('reopened') is True and seed.get('revision',0)>0,'Opaque seed not recovered through journal')
    def opaque(value):
        if isinstance(value,list):return [n for v in value for n in opaque(v)]
        if isinstance(value,dict):return ([value] if value.get('type') in ('user','link') else [])+[n for v in value.values() for n in opaque(v)]
        return []
    require(len(opaque(seed.get('blocks')))==2 and opaque(original)==opaque(seed['blocks']),'Existing opaque content changed')
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Missing canonical inventory')
    counts=[(0,0)]*7+[(1,1),(1,1),(2,1),(3,2),(4,3),(4,3),(4,3),(4,3),(4,3),(4,3),(4,3),(4,3)]
    prefix=[];prepared=[];reads=[];view=None;retained=None
    initial=rows[0]['model']
    for i,row in enumerate(rows):
        name=row['stage'];model=row['model'];g=row['geometry'];ledger=model.get('composeLedger',{})
        require(model.get('noSession') is True and model.get('noTransport') is True and model.get('writes')==model.get('networkCalls')==0,'Live state or unrelated write introduced')
        require(model.get('initialDocument')==initial['initialDocument'] and model.get('initialRecords')==touched and model.get('initialUnrelated')==unrelated,'Original canonical evidence changed')
        if name=='closed':
            require(model.get('unrelated') is None and model.get('document') is None
                    and model.get('records') is None and isinstance(model.get('comments'),dict)
                    and model['comments'].get('active') is False and model['comments'].get('loading') is False,'Closed List retained an active model')
        elif name=='account-replaced':
            require(model.get('unrelated') is None and model.get('document') is None and model.get('comments') is None
                    and model.get('epoch',0)>initial.get('epoch',0) and model.get('account')!=initial.get('account'),'Replacement retained predecessor surface')
        else:
            expected=__import__('copy').deepcopy(touched)
            if name in ('record-replaced','account-held'):
                estimate=[f for f in expected[READ['ALIAS']]['fields'] if f['key']=='estimate']
                require(len(estimate)==1,'Missing original estimate field');estimate[0]['number']=[19]
            require(model.get('unrelated')==unrelated and model.get('document')==initial['initialDocument']
                    and model.get('records')==expected,'Canonical source changed outside admitted replacement')
        require(model.get('ledger',{}).get('requests',[])[:len(reads)]==reads,'Read ledger rewritten')
        reads=model['ledger']['requests'];validate_reads(reads)
        posts=ledger.get('posts');current=ledger.get('prepared')
        require(isinstance(posts,list) and isinstance(current,list) and (len(current),len(posts))==counts[i],'Duplicate/missing reply attempt')
        require(posts[:len(prefix)]==prefix and current[:len(prepared)]==prepared,'Original outgoing ledger replaced')
        prefix=posts;prepared=current
        for out in current:outgoing(out)
        require(len({p['args']['client_msg_id'] for p in current})==len(current),'UUID replay')
        if posts:require(posts[0]==current[0] and outgoing(posts[0])==original,'Initial send differs from exact native blocks')
        if len(posts)>1:require(posts[1:]==current[2:],'Known-unsent request reached dispatch')
        for req in ledger.get('preflights',[]):
            require(req=={'method':'conversations.genericInfo','args':{'updated_channels':'{"CCOMMENT01":0}'}},'Preflight channel request changed')
        for policy in ledger.get('policies',[]):
            require(policy.get('request')=={'method':'client.userBoot','args':{'include_min_version_bump_check':'1'}}
                    and policy.get('boot',{}).get('ok') is True and policy.get('channel',{}).get('id')=='CCOMMENT01'
                    and type(policy.get('threadReadOnly')) is bool,'Posting authority not observed')
        require(len(ledger.get('policies',[]))>=len(current),'Attempt lacked fresh posting authority')
        messages=ledger.get('messages',[])
        expected_posts=[posts[n] for n in (0,2) if n<len(posts)]
        require(len(messages)==len(expected_posts),'Unexpected positive fixture message')
        for message,post in zip(messages,expected_posts):
            a=post['args']
            require(message.get('channel')==a['channel'] and message.get('thread_ts')==a['thread_ts']
                    and message.get('client_msg_id')==a['client_msg_id'] and message.get('blocks')==json.loads(a['blocks'])
                    and isinstance(message.get('user'),str) and message['user'] and message.get('ts')!=a['thread_ts'],'Positive message lost original request witness')
        require(ledger.get('held') is (name in ('record-held','account-held')) and ledger.get('heldPost') is (name=='sending'),'Wrong held work boundary')
        if name in ('sending','sent','not-sent','rejected','unconfirmed','locked-reopen','confirmed'):
            comments=model.get('comments') or {}
            visible=comments.get('visibleTs')
            dom=[r.get('ts') for r in g.get('rows',[])]
            require(comments.get('active') is True and comments.get('record')==READ['KNOWN']
                    and comments.get('root')==READ['ROOT_TS'] and isinstance(visible,list)
                    and all(isinstance(ts,str) and ts for ts in visible)
                    and all(isinstance(r.get('text'),str) for r in g.get('rows',[]))
                    and len(set(visible))==len(visible) and len(set(dom))==len(dom) and dom==visible,
                    'Visible comment ownership/order/uniqueness changed')
            before=rows[7]['model']['comments']['visibleTs']
            first_ts=messages[0]['ts']
            if name=='sending':
                require(first_ts not in visible,'Held send projected before positive acceptance')
            elif name=='sent':
                require(first_ts not in before and sorted(visible)==sorted(before+[first_ts])
                        and visible.count(first_ts)==1,'Accepted first comment not immediately projected exactly once')
                require(reads==rows[7]['model']['ledger']['requests'],'Immediate accepted display required an extra read')
                require(all(comments.get(k)==rows[7]['model']['comments'].get(k) for k in ('generation','page','hasMore')),
                        'Immediate accepted overlay replaced original page/cursor/visit')
                reveal=g.get('reveal') or {}
                require(reveal.get('ts')==first_ts and isinstance(reveal.get('serial'),str)
                        and re.fullmatch('[1-9][0-9]*',reveal['serial']), 'Accepted comment reveal identity missing')
                scroll=reveal.get('scrollRect') or {}
                require(all(type(scroll.get(k)) in (int,float) and math.isfinite(scroll[k])
                            for k in ('left','top','right','bottom','width','height'))
                        and scroll['height']>=80 and scroll['width']>0
                        and abs(scroll['bottom']-scroll['top']-scroll['height'])<1
                        and abs(scroll['right']-scroll['left']-scroll['width'])<1
                        and type(reveal.get('scrollTop')) in (int,float) and math.isfinite(reveal['scrollTop'])
                        and reveal['scrollTop']>=0, 'Accepted comment reveal geometry missing')
                accepted=next(r for r in g['rows'] if r['ts']==first_ts)
                height=accepted.get('visibleHeight')
                measured=accepted.get('rect') or {}
                require(all(type(measured.get(k)) in (int,float) and math.isfinite(measured[k])
                            for k in ('left','top','right','bottom','width','height'))
                        and measured['height']>0 and measured['width']>0
                        and abs(measured['bottom']-measured['top']-measured['height'])<1
                        and abs(measured['right']-measured['left']-measured['width'])<1,
                        'Accepted comment measurement missing')
                intersection=max(0,min(measured['bottom'],scroll['bottom'],g['viewport'][1])-max(measured['top'],scroll['top'],0))
                require(type(height) in (int,float) and math.isfinite(height) and height>=0
                        and abs(height-intersection)<1
                        and height>=min(measured['height'],80)-1, 'Accepted comment body was not readably revealed in the transcript')
            elif name in ('not-sent','rejected','unconfirmed'):
                require(visible==rows[8]['model']['comments']['visibleTs'],
                        'Failed/uncertain send projected or removed an accepted comment')
                require([(r['ts'],r.get('text')) for r in g['rows']]
                        ==[(r['ts'],r.get('text')) for r in rows[8]['geometry']['rows']],
                        'Failed/uncertain payload replaced accepted transcript content')
            elif name=='locked-reopen':
                require(set(visible) in (set(before),set(before+[first_ts])),
                        'Unconfirmed payload projected on reopen')
            elif name=='confirmed':
                page=(comments.get('page') or {}).get('messages',[])
                require(isinstance(page,list),'Confirmed readback page missing')
                for message in messages:
                    matching=[m for m in page if m.get('ts')==message['ts']]
                    require(visible.count(message['ts'])==1 and dom.count(message['ts'])==1 and len(matching)==1,
                            'Accepted/published message missing or duplicated in confirmed readback')
                    require(all(matching[0].get(k)==message.get(k) for k in ('client_msg_id','blocks','user','thread_ts')),
                            'Confirmed readback changed accepted message identity')
        require(g.get('ok') is True and g.get('noOverflow') is True,'Compose layout overflow')
        viewport=g.get('viewport');require(isinstance(viewport,list) and len(viewport)==2,'Missing viewport')
        if view is None:view=viewport
        require(viewport==view,'Viewport changed')
        controls=g.get('controls',[]);require(len({c['id'] for c in controls})==len(controls),'Duplicate controls')
        if name=='sent' and model['comments'].get('hasMore') is True:
            require(any(c['id']=='slack-list-comments-next' for c in controls),'Readable transcript lost Next page control')
        for c in controls:
            require(c.get('visible') is True and c.get('hit') is True,'Control obscured');rectangle(c.get('rect'),viewport)
        e=g.get('editor')
        store=model.get('draftStore',{});drafts=store.get('drafts',[])
        require(isinstance(drafts,list) and drafts,'Original draft custody missing')
        for draft in drafts:
            require(draft.get('captureFailed') is False and draft.get('terminalPending') is False
                    and draft.get('savedRevision',0)>0,'Draft persistence unresolved')
        if e:
            require(any(d['blocks']==e['blocks'] for d in drafts),'Native draft differs from retained store')
        if name in ('sending','unconfirmed','locked-reopen'):
            require(len(drafts)==1 and drafts[0].get('attempt') is not None,'Durable outgoing attempt missing')
            attempt=drafts[0]['attempt']['record'];issued=posts[0 if name=='sending' else -1]['args']
            require(attempt['client_msg_id']==issued['client_msg_id'] and attempt['blocks']==json.loads(issued['blocks']),'Durable attempt lost exact request')
        elif name not in ('record-held','record-replaced','account-held','account-replaced'):
            require(all(d.get('attempt') is None for d in drafts),'Settled attempt remains locked')
        if name in ('close-reopen','locked-reopen'):
            predecessor=rows[3 if name=='close-reopen' else 11]['model']['draftStore']['drafts'][0]
            require(row['proof'].get('runtimeForgotten') is True and len(drafts)==1
                    and drafts[0]['id']!=predecessor['id'] and drafts[0]['capture']==predecessor['capture']
                    and drafts[0]['blocks']==predecessor['blocks'] and drafts[0].get('attempt')==predecessor.get('attempt'),'Journal recovery did not restore original persisted state')
        if name=='account-replaced':
            prior=rows[-2]['model']['draftStore']['drafts']
            require([(d['id'],d['scope'],d['blocks'],d['capture']) for d in drafts]
                    ==[(d['id'],d['scope'],d['blocks'],d['capture']) for d in prior],'Account retirement lost original draft owner')
        if e:
            require(e.get('visible') is True,'Editor clipped');rectangle(e.get('rect'),viewport)
            require(isinstance(e.get('blocks'),list),'Native blocks unavailable')
        if name in ('draft','back-reopen','close-reopen','escape-reopen','scrim-reopen','sending'):
            require(e and e['blocks']==original and e.get('bold') is True and '\n' in e.get('text',''),'Rich draft changed across departure')
        if name in ('back','closed','record-replaced','account-replaced'):require(e is None,'Retired editor still mounted')
        if name in ('closed','account-replaced'):require(g.get('dialog') is False,'Dialog failed to depart')
        if name=='back':require(g.get('active')=='slack-list-record-comments','Back focus not restored')
        if name=='not-sent':retained=e['blocks']
        if name in ('not-sent','rejected','unconfirmed','locked-reopen'):
            require(e and e['blocks']==retained,'Failure lost exact native draft')
        if name in ('sending','unconfirmed','locked-reopen','record-held','account-held'):
            require(e is not None,'Pending/uncertain draft missing')
            if name in ('sending','record-held','account-held'):require(e.get('editable') is False,'Pending draft remained editable')
            require(any(c['id']=='slack-list-comment-send' and c.get('disabled') is True for c in controls),'Pending/uncertain send unlocked')
        if name in ('not-sent','rejected','denied'):
            require(e and e.get('editable') is True and g.get('notices'),'Recoverable failure not visible')
        if name in ('unconfirmed','locked-reopen'):require('unconfirmed' in ' '.join(g.get('notices',[])).lower(),'Uncertainty guidance missing')
        if name=='sent':
            require(row['proof']['release']==dict(delivered=True,cancelledBeforeReply=False) and (not e or not e['text'].strip()),'Success not positively settled')
        if name=='unconfirmed':require(row['proof']['release']==dict(delivered=True,cancelledBeforeReply=False),'Issued receipt lost original owner')
        if name=='confirmed':
            require(ledger.get('published') is True and (not e or not e['text'].strip()),'Positive readback did not clear sent draft')
        if name in ('record-replaced','account-replaced'):
            require(row['proof'].get('release')==dict(delivered=False,cancelledBeforeReply=True),'Retired preflight still delivered')
        capture=directory/f'{fixture}-{name}.png';require(Path(row.get('capture',''))==capture,'Foreign capture path')
        if png:H['png_signature'](capture,dimensions)
    require(receipt.get('final')==rows[-1]['model'],'Terminal model changed')
    require(len(reads)>=4 and len(rows[-1]['model']['composeLedger']['preflights'])>=7,'Missing actual read/preflight traffic')
    readback=receipt.get('acceptedReadback') or {}
    refreshed=readback.get('model') or {}; geometry=readback.get('geometry') or {}
    sent=rows[8]['model']; accepted=sent['composeLedger']['messages'][0]
    require(all(refreshed.get(k)==sent.get(k) for k in ('account','epoch','noSession','noTransport','writes','networkCalls',
            'document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail')),
            'Immediate readback lost original source/owner')
    requests=(refreshed.get('ledger') or {}).get('requests',[])
    sent_requests=sent['ledger']['requests']
    require(requests[:len(sent_requests)]==sent_requests and len(requests)==len(sent_requests)+4
            and [r['method'] for r in requests[len(sent_requests):]]==['files.info','lists.records.info','experiments.getByUser','conversations.replies'],
            'Same-panel readback did not perform one explicit refresh')
    validate_reads(requests)
    require(requests[-3]['args']['id']==READ['KNOWN'] and requests[-1]['args']['ts']==READ['ROOT_TS'],
            'Readback was not the original same-panel refresh')
    # The next Send performs its own fresh files/record/experiments/replies preflight.
    # Keep that acquisition distinct from the preceding explicit Refresh.
    next_reads=rows[9]['model']['ledger']['requests']
    require(next_reads[:len(requests)]==requests and len(next_reads)==len(requests)+4
            and next_reads[len(requests):]==[dict(request,serial=request['serial']+4) for request in requests[-4:]],
            'Next Send did not append its exact original-scope four-read preflight')
    expected_ledger=dict(sent['composeLedger'],published=True)
    require(refreshed.get('composeLedger')==expected_ledger,'Same-panel refresh sent or replaced a comment')
    comments=refreshed.get('comments') or {}; original_comments=sent['comments']
    require(all(comments.get(k)==original_comments.get(k) for k in ('generation','active','record','root'))
            and comments.get('loading') is False and comments.get('error') is None,
            'Same-panel readback retired the original comments visit')
    visible=comments.get('visibleTs'); dom=geometry.get('rows',[])
    require(geometry.get('ok') is True and geometry.get('noOverflow') is True
            and geometry.get('viewport')==rows[8]['geometry']['viewport']
            and visible==original_comments['visibleTs'] and [r.get('ts') for r in dom]==visible
            and len(visible)==len(set(visible)) and visible.count(accepted['ts'])==1,
            'Same-panel readback duplicated/lost the accepted comment')
    page=comments.get('page') or {}; original_page=original_comments.get('page') or {}
    require(page.get('has_more')==original_page.get('has_more') and comments.get('hasMore')==original_comments.get('hasMore')
            and (page.get('response_metadata') or {}).get('next_cursor')==(original_page.get('response_metadata') or {}).get('next_cursor'),
            'Accepted overlay changed source pagination')
    matched=[m for m in page.get('messages',[]) if m.get('ts')==accepted['ts']]
    require(len(matched)==1 and sum(m.get('client_msg_id')==accepted['client_msg_id'] for m in page.get('messages',[]))==1
            and all(matched[0].get(k)==accepted.get(k) for k in ('client_msg_id','blocks','user','thread_ts')),
            'Same-panel page lacks exactly one original accepted message')
    drafts=(refreshed.get('draftStore') or {}).get('drafts',[])
    prior=sent['draftStore']['drafts']
    require(len(drafts)==len(prior)==1 and all(drafts[0].get(k)==prior[0].get(k) for k in ('id','scope','capture'))
            and drafts[0].get('blocks')==[] and drafts[0].get('attempt') is None and drafts[0].get('terminalPending') is False
            and (not geometry.get('editor') or (geometry['editor'].get('blocks')==[] and not geometry['editor'].get('text','').strip())),
            'Same-panel refresh restored or replaced the sent draft')
    return receipt


REACTION_STAGES=('draft','added','duplicate','removed','foreign','sent','tail-added','tail-removed')
REACTION_TEXT='Final review \u4e2d\U0001f642\nSecond line <literal> & *punctuation*.'


def validate_reactions(fixture,receipt,directory,*,dimensions=None,png=True):
    """Parsed realtime events change only original loaded/tail reaction projections."""
    import math
    rich=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_newer_draft.py')))
    directory=Path(directory)
    require(fixture in FIXTURES and receipt.get('ok') is True and receipt.get('fixture')==fixture
            and receipt.get('case')=='reactions' and receipt.get('noLiveNetwork') is True,'Wrong reactions receipt')
    ready=receipt.get('readiness',{})
    H['validate_ready'](ready,ready_tuple(fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt.get('stages',[])
    require([r.get('stage') for r in rows]==list(REACTION_STAGES),'Missing reaction stages')
    first=rows[0];initial=first['model'];original=first['proof']['blocks'];seed=first['proof']['seed']
    touched,unrelated=READ['record_proof'](receipt.get('initialRecords'))
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Missing canonical inventory')
    require(initial['initialDocument'].get('id')==READ['LIST']
            and touched[READ['KNOWN']].get('thread_ts')==READ['ROOT_TS'],'Wrong original List/root')
    scope=seed.get('scope',{})
    require(set(scope)=={'workspace','user','list','record','created','creator','root'}
            and scope['workspace']=='T1' and scope['user']=='U0' and scope['list']==READ['LIST']
            and scope['record']==READ['KNOWN'] and scope['root']==READ['ROOT_TS']
            and scope['created']==touched[READ['KNOWN']].get('date_created')
            and scope['creator']==touched[READ['KNOWN']].get('created_by'),'Wrong draft owner')
    require(seed.get('reopened') is True and rich['integer'](seed.get('revision'),1),'Seed not restored from journal')
    require(isinstance(original,list) and original and len(rich['opaque'](seed.get('blocks')))==2
            and rich['opaque'](original)==rich['opaque'](seed['blocks']),'Opaque rich seed changed')
    paragraphs=rich['paragraphs'](original)
    require(len(paragraphs)==2 and paragraphs[0][1].endswith(REACTION_TEXT.split('\n')[0]+'\n')
            and paragraphs[1][1]==REACTION_TEXT.split('\n')[1]
            and any(style.get('bold') is True for _,style in rich['text_units'](original)),
            'Native rich fixture text/style changed')
    original_journal=first['proof']['journal'];sent_journal=rows[5]['proof']['journal']
    original_draft=initial['draftStore']['drafts'][0];sent_draft=rows[5]['model']['draftStore']['drafts'][0]
    require(sent_draft['generation']!=original_draft['generation'],'Successful send did not rotate editor')
    require(sent_journal['revision']>original_journal['revision'] and sent_journal['capture']!=original_journal['capture'],
            'Successful send did not settle durable capture')
    first_ts='1781524801.000001';channel='CCOMMENT01'
    require(initial['comments']['visibleTs']==[first_ts],'Original loaded comment changed')
    reads0=initial['ledger']['requests'];reads_sent=rows[5]['model']['ledger']['requests']
    require(len(reads0)==4 and len(reads_sent)==8 and reads_sent[:4]==reads0,'Extra/missing read before send')
    require([r['method'] for r in reads0]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']
            and [dict(r,serial=i+1) for i,r in enumerate(reads_sent[4:])]==reads0,'Send preflight changed original read factories')
    posts=rows[5]['model']['composeLedger']['posts']
    require(len(posts)==1 and outgoing(posts[0])==original,'Send did not use exact native blocks')
    positive=rows[5]['model']['composeLedger']['messages']
    require(len(positive)==1,'Missing unique positive accepted message')
    rich['_message'](positive[0],posts[0],scope)
    tail_ts=positive[0]['ts']
    require(tail_ts not in initial['comments']['visibleTs'],'Accepted timestamp reused')
    viewport=None
    for index,row in enumerate(rows):
        name=row['stage'];model=row['model'];g=row['geometry'];proof=row['proof'];comments=model['comments']
        sent=index>=5;expected_blocks=[] if sent else original;generation=sent_draft['generation'] if sent else original_draft['generation']
        require(proof.get('seed')==seed and proof.get('blocks')==original,'Original rich proof changed')
        require(model.get('account')==initial.get('account') and rich['uuid'](model.get('account'))
                and model.get('epoch')==initial.get('epoch') and rich['integer'](model.get('epoch'))
                and model.get('noSession') is True and model.get('noTransport') is True
                and type(model.get('writes')) is int and model['writes']==0
                and type(model.get('networkCalls')) is int and model['networkCalls']==0,'Owner/isolation changed')
        require(model.get('document')==model.get('initialDocument')==initial['initialDocument']
                and model.get('records')==model.get('initialRecords')==touched
                and model.get('unrelated')==model.get('initialUnrelated')==unrelated,'Canonical source mutated')
        require(comments.get('active') is True and comments.get('loading') is False and comments.get('error') is None
                and comments.get('record')==READ['KNOWN'] and comments.get('root')==READ['ROOT_TS']
                and model.get('detail')==initial['detail']
                and all(comments.get(k)==initial['comments'].get(k) for k in ('generation','page','hasMore','requestTs'))
                and comments.get('hasMore') is True,'Reaction disturbed owner/page/pagination')
        reads=model['ledger']['requests'];validate_reads(reads)
        require(reads==(reads_sent if sent else reads0),'Reaction issued or rewrote a read')
        require(model['ledger'].get('held') is False and model['ledger'].get('completed')==len(reads)
                and model['ledger'].get('cancelled')==0,'Read work remained unresolved')
        ledger=model['composeLedger'];count=int(sent)
        require(ledger.get('prepared')==ledger.get('posts')==(posts if sent else [])
                and ledger.get('messages')==(positive if sent else []) and ledger.get('published') is False
                and ledger.get('held') is False and ledger.get('heldPost') is False,'Extra POST/readback or changed accepted payload')
        preflights=ledger.get('preflights',[]);policies=ledger.get('policies',[])
        require(len(preflights)==len(policies)==count,'Missing/extra posting observation')
        if sent:
            require(ledger==rows[5]['model']['composeLedger'],'Reaction rewrote send ledger')
            require(preflights[0]=={'method':'conversations.genericInfo','args':{'updated_channels':'{"CCOMMENT01":0}'}},'Wrong channel admission')
            p=policies[0]
            require(p.get('request')=={'method':'client.userBoot','args':{'include_min_version_bump_check':'1'}}
                    and p.get('boot',{}).get('ok') is True and p['boot'].get('team',{}).get('id')==scope['workspace']
                    and p['boot'].get('self',{}).get('id')==scope['user'] and p.get('channel',{}).get('id')==channel
                    and p.get('threadReadOnly') is False,'Posting authority changed')
        store=model['draftStore'];drafts=store.get('drafts',[])
        require(len(drafts)==1,'Draft custody changed')
        draft=drafts[0];journal=proof.get('journal')
        require(journal==(sent_journal if sent else original_journal)
                and journal.get('scope')==draft.get('scope')==scope
                and journal.get('blocks')==draft.get('blocks')==expected_blocks
                and journal.get('capture')==draft.get('capture') and rich['uuid'](draft.get('capture'))
                and journal.get('revision')==draft.get('savedRevision') and rich['integer'](journal.get('revision'),1)
                and journal.get('attempt') is None and draft.get('attempt') is None,'Native model and fresh journal disagree')
        require(draft.get('id')==original_draft['id'] and draft.get('generation')==generation
                and store.get('active')==f"{draft['id']}:{comments['generation']}:{generation}"
                and all(draft.get(k) is False for k in ('busy','saving','loading','captureFailed','terminalPending'))
                and draft.get('error') is None,'Reaction disturbed draft binding/storage')
        require(g.get('ok') is True and g.get('noOverflow') is True and g.get('dialog') is True
                and g.get('comments') is True and g.get('compose') is True,'Reaction layout unavailable')
        v=g.get('viewport')
        require(isinstance(v,list) and len(v)==2 and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in v),'Invalid viewport')
        if viewport is None:viewport=v
        require(v==viewport,'Viewport changed')
        controls=g.get('controls',[]);ids=[c['id'] for c in controls]
        require(len(set(ids))==len(ids) and {'slack-list-comments-back','slack-list-comments-refresh','slack-list-comments-next','slack-list-comment-send'}<=set(ids),'Missing/duplicate control')
        for c in controls:
            require(c.get('visible') is True and c.get('hit') is True,'Control obscured');rectangle(c.get('rect'),v)
            require(c.get('disabled') is (sent if c['id']=='slack-list-comment-send' else False),'Control admission changed')
        e=g.get('editor')
        require(isinstance(e,dict) and e.get('visible') is True and e.get('editable') is True
                and e.get('blocks')==expected_blocks and e.get('generation')==str(generation)
                and type(e.get('sameNode')) is bool and (sent or e['sameNode'] is True)
                and g.get('active')=='slack-list-comment-draft',
                'Reaction lost original native node/focus/content')
        rectangle(e.get('rect'),v)
        # An empty contenteditable paragraph serializes innerText as a single LF.
        # Accept that scaffold only after exact empty native/model/disk blocks above.
        native_text=e.get('text')
        require((native_text in ('','\n') if sent else native_text==rich['visible_text'](original))
                and (sent or e.get('bold') is True),
                'Native text/style changed')
        visible=initial['comments']['visibleTs']+([tail_ts] if sent else [])
        dom=g.get('rows',[])
        require(comments.get('visibleTs')==visible and [r.get('ts') for r in dom]==visible
                and len(set(visible))==len(visible),'Comment projection missing/duplicated')
        # Row text includes its changing reaction count. Raw Page/accepted payload
        # equality above protects content; do not mistake pill text for a body edit.
        require(all(isinstance(r.get('text'),str) and r['text'] for r in dom),'Missing transcript text')
        target=tail_ts if sent else first_ts;expected_count=(2,3,3,2,2,0,1,0)[index]
        for item in dom:
            reactions=item.get('reactions')
            require(isinstance(reactions,list) and all(set(r)=={'name','count'} and isinstance(r['name'],str)
                    and rich['integer'](r['count'],1) for r in reactions)
                    and len({r['name'] for r in reactions})==len(reactions),'Malformed DOM reaction pill')
            expected=expected_count if item['ts']==target else 2
            require(reactions==([{'name':'wave','count':expected}] if expected else []),'DOM reaction count wrong')
        probe=row.get('reactionProbe')
        require(comments.get('reactions')==[dict(ts=r['ts'],reactions=r['reactions']) for r in dom],
                'Model and DOM reaction projections disagree')
        require(probe==dict(ts=target,count=expected_count,channel=channel,editorGeneration=str(generation),
                hasMore=True,readRequests=len(reads),prepared=count,posts=count),'Model reaction probe disagrees')
        require(all(type(probe[k]) is int for k in ('count','readRequests','prepared','posts'))
                and probe['hasMore'] is True,'Reaction probe scalar types changed')
        capture=directory/f'{fixture}-{name}.png'
        require(Path(row.get('capture',''))==capture,'Foreign capture path')
        if png:H['png_signature'](capture,dimensions)
    events=receipt.get('reactionEvents');expected=[]
    for stage,team,generation,event_channel,ts,added,changed in (
        ('added','T1',7,channel,first_ts,True,True),('duplicate','T1',7,channel,first_ts,True,False),
        ('removed','T1',7,channel,first_ts,False,True),('foreign','T1',7,'COTHER',first_ts,True,False),
        ('foreign','T_OTHER',7,channel,first_ts,True,False),('foreign','T1',6,channel,first_ts,True,False),
        ('foreign','T1',7,channel,'1999999999.000001',True,False),
        ('tail-added','T1',7,channel,tail_ts,True,True),('tail-removed','T1',7,channel,tail_ts,False,True)):
        expected.append(dict(stage=stage,team=team,generation=generation,changed=changed,
            frame=dict(type='reaction_added' if added else 'reaction_removed',user='U_RT_REACTOR',reaction='wave',
                       item=dict(type='message',channel=event_channel,ts=ts))))
    require(events==expected,'Parsed event routing evidence changed')
    require(all(type(e['generation']) is int and type(e['changed']) is bool for e in events),
            'Event generation/change types changed')
    require(receipt.get('final')==rows[-1]['model'],'Final model changed')
    return receipt


def case_contract(case):
    require(case in ('compose','newer-draft','storage-recovery','reactions','reaction-refresh','deletions','incoming',
                     'ack-socket-add','ack-socket-remove','ack-first','edits','edit-ack'), 'Unknown compose acceptance case')
    if case in ('edits','edit-ack'):
        module=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_edits.py')))
        return module['STAGES'][case],module['validator'](case)
    if case in ('ack-socket-add','ack-socket-remove','ack-first'):
        module=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_ack_socket.py')))
        return module['STAGES'][case],module['validator'](case)
    if case=='reactions':return REACTION_STAGES,validate_reactions
    if case!='compose':
        name={'newer-draft':'slack_list_comments_newer_draft.py','storage-recovery':'slack_list_comments_storage_recovery.py',
              'reaction-refresh':'slack_list_comments_reaction_refresh.py','deletions':'slack_list_comments_deletions.py',
              'incoming':'slack_list_comments_incoming.py'}[case]
        module=runpy.run_path(str(Path(__file__).with_name(name)))
        return module['STAGES'],module['validate']
    return STAGES,validate


def validate_log(native,fixture,directory,expected,dimensions,case='compose'):
    waiting,receipts=native.receipts(WAITING),native.receipts(PREFIX)
    require(len(waiting)==len(receipts)==1,'Missing/duplicate ownership or terminal receipt')
    H['validate_ready'](waiting[0],expected)
    require(receipts[0].get('ok') is True,'Native compose failed: '+str(receipts[0].get('error')))
    H['validate_ready'](receipts[0].get('readiness'),expected)
    stages,validator=case_contract(case)
    require(native.receipts(EDGE)==[dict(stage=s,edge=e) for s in stages for e in ('before','after')],'Capture lifecycle edges changed')
    return validator(fixture,receipts[0],directory,dimensions=dimensions)

def run_fixture(fixture, binary, captures, case='compose'):
    require(fixture in FIXTURES, 'Unknown List comments fixture')
    case_contract(case)
    captures.mkdir(parents=True, exist_ok=True)
    nonce = secrets.token_hex(16)
    directory = captures / f'slack-list-comments-compose-{nonce}'
    directory.mkdir(exist_ok=False)
    (directory/'owner.json').write_text(json.dumps(dict(version=1,nonce=nonce,fixture=fixture)),encoding='utf-8')
    with binary.open('rb') as source:
        digest = hashlib.file_digest(source, 'sha256').hexdigest()
    evidence = dict(fixture=fixture, case=case, nonce=nonce, binary=str(binary), binarySha256=digest, ok=False,
                    readyDirectory=str(directory), limitations=['Placement and pointer checks are sampled observations, not atomic capture proof.'])
    native = NativeFixture(fixture, captures / f'{fixture}.log', binary,
                           {FLAG:'1','SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':case,'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_NONCE':nonce,
                            'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_READY_DIR':str(directory),
                            'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CAPTURE_DIR':str(directory),
                            'SUPER_PLATINUM_LIST_COMMENT_JOURNAL':str(directory/'comments.sqlite')})
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
                    storage = None
                    if case == 'storage-recovery':
                        controller=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_storage_lock.py')))
                        storage=stack.enter_context(controller['StorageControl'](directory,expected))
                        evidence['storage']=storage.evidence
                    H['publish_ready'](directory, expected)
                    evidence['readiness'] = expected
                    deadline, sampled, count = time.monotonic() + 180, 0., 0
                    while True:
                        native.require_running()
                        if storage:
                            storage.service(native)
                        edges = native.receipts(EDGE)
                        if pointer:
                            for edge in edges[count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check('running', require_clear=True)
                                sampled = time.monotonic()
                        count = len(edges)
                        if native.receipts(PREFIX):
                            evidence['native'] = validate_log(native, fixture, directory, expected, evidence.get('expectedPngDimensions'),case)
                            if storage:
                                storage.finish(native)
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
        require(validate_log(native, fixture, directory, expected, evidence.get('expectedPngDimensions'),case) == evidence['native'], 'Closed log changed')
        if storage:
            storage.validate_closed(native)
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
    parser.add_argument('--captures',type=Path,default=ROOT/'tmp/list-comments-compose-native')
    parser.add_argument('--fixture',action='append',choices=FIXTURES)
    parser.add_argument('--case',choices=('compose','newer-draft','storage-recovery','reactions','reaction-refresh','deletions','incoming',
                                        'ack-socket-add','ack-socket-remove','ack-first','edits','edit-ack'),default='compose')
    args=parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        captures=args.captures.resolve()/fixture
        require(not captures.exists(),'Use a fresh capture directory')
        run_fixture(fixture,args.binary.resolve(),captures,args.case)

if __name__=='__main__':
    main()
