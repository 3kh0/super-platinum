"""Bounded adjacent text-run equality; captured rich values remain untouched."""
import copy
from pathlib import Path
import runpy

P=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_noop.py')))
O,T,N,G,R,C,READ,H,S=(P[k] for k in ('O','T','N','G','R','C','READ','H','S'))
require,exact=P['require'],P['exact']
CASE='own-edit-runs'
STAGES=('draft','editing','formatted','reverted','returned','changed-edit','saving','saved','readback')
ORIGINAL=copy.deepcopy(P['ORIGINAL'])
READS=(4,8,8,8,8,12,16,16,20)
POLICIES=(0,1,1,1,1,2,3,3,3)


def text_run(value):
    if not isinstance(value,dict) or value.get('type')!='text' or not set(value)<= {'type','text','style'}:
        return False
    text=value.get('text')
    if not isinstance(text,str) or not text or '\n' in text or '\r' in text:return False
    if 'style' not in value:return True
    style=value['style']
    return isinstance(style,dict) and set(style)<= {'bold','italic','strike','code'} and all(type(v) is bool for v in style.values())


def coalesced(value):
    """Copy only. Semantic/opaque/newline leaves remain exact merge barriers."""
    def container(node):
        result=copy.deepcopy(node)
        if not isinstance(node,dict) or not isinstance(node.get('elements'),list):return result
        if node.get('type') in ('rich_text','rich_text_list'):
            result['elements']=[container(v) for v in node['elements']]
        elif node.get('type')=='rich_text_section':
            leaves=[]
            for leaf in result['elements']:
                if leaves and text_run(leaf) and text_run(leaves[-1]) and ('style' in leaf)==('style' in leaves[-1]) and exact(leaf.get('style'),leaves[-1].get('style')):
                    leaves[-1]['text']+=leaf['text']
                else:leaves.append(leaf)
            result['elements']=leaves
        return result
    return [container(v) for v in value] if isinstance(value,list) else copy.deepcopy(value)


def segmentation(blocks):
    require(not exact(blocks,ORIGINAL),'Actual rich capture did not change leaf segmentation')
    require(exact(coalesced(blocks),coalesced(ORIGINAL)),'Formatting revert changed more than adjacent ordinary same-style runs')


def retained_italic(blocks):
    expected=copy.deepcopy(ORIGINAL)
    expected[0]['elements'][0]['elements']=[dict(type='text',text='Original ',style={'bold':True}),
        dict(type='text',text='own',style={'bold':True,'italic':True}),
        dict(type='text',text=' comment 中🙂.',style={'bold':True})]
    require(exact(coalesced(blocks),coalesced(expected)) and not exact(coalesced(blocks),coalesced(ORIGINAL)),
        'Retained formatting did not change only original selected own substring')


def format_owner(value,mount,blocks,action=None):
    require(isinstance(value,dict) and value.get('sameNode') is True and value.get('scope')==mount['scope']
        and value.get('generation')==mount['generation'] and value.get('selected')=='own'
        and exact(value.get('native',{}).get('blocks'),blocks),'Formatting lost original selection/native owner')
    if action is not None:require(value.get('ok') is True and value.get('action')==action,'Wrong formatting action')


def selection(value,mount):
    format_owner(value,mount,ORIGINAL,'select')
    require(value.get('textNode')==ORIGINAL[0]['elements'][0]['elements'][0]['text']
        and exact(value.get('anchorOffset'),9) and exact(value.get('focusOffset'),12),'Selection did not target original own substring')


def click(value,mount,before,viewport):
    require(value.get('ok') is True and value.get('action')=='italic','Missing actual original Italic click')
    format_owner(value['before'],mount,before)
    c=value['control'];require(c.get('label')=='Italic' and c.get('disabled') is False and c.get('sameNode') is True
        and c.get('visible') is True and c.get('hit') is True,'Italic button was replaced/disabled/covered')
    N['rectangle'](c.get('rect',{}),viewport)


def native(g,index,reply,blocks):
    editing=index in (1,2,3,5,6);saving=index==6
    viewport=g.get('viewport')
    require(g.get('ok') is True and g.get('noOverflow') is True and g.get('editing') is editing
        and isinstance(viewport,list) and len(viewport)==2 and all(R['integer'](v,1) for v in viewport)
        and g.get('captureFault') is None,'Invalid original native geometry')
    ids=['slack-list-comment-edit-cancel','slack-list-comment-edit-save'] if editing else ['slack-list-comments-back','slack-list-comment-send']
    require([v.get('id') for v in g['controls']]==ids,'Native controls omitted/replaced')
    for c in g['controls']:
        require(c.get('disabled') is saving and c.get('visible') is True and c.get('hit') is True,'Native control enabled/covered incorrectly')
        N['rectangle'](c.get('rect',{}),viewport)
    if saving:O['inert_text'](g.get('text'),g.get('inertTextPaint'),viewport)
    else:
        require(g.get('inertTextPaint') is None,'Saving paint exception outside held Save')
        N['text_proof'](g.get('text'),viewport)
    r=g['reply'];require(exact(r.get('blocks'),reply) and r.get('sameNode') is True and r.get('composing') is False
        and r.get('visible') is (not editing) and r.get('hidden') is editing and r.get('inert') is editing
        and g.get('replyNotice') is None,'Original independent reply lost custody')
    if editing:
        e=g['edit'];require(exact(e.get('blocks'),blocks) and e.get('sameNode') is True and e.get('editable') is (not saving),'Original edit buffer/node changed')
        N['rectangle'](e.get('rect',{}),viewport)
    else:
        require(g.get('edit') is None,'Closed editor survived');N['rectangle'](r.get('rect',{}),viewport)
    if saving:
        s=g['notice'];require(s.get('text')=='Saving the edited comment…' and s.get('visible') is True and s.get('hit') is True,'Held Save guidance missing')
        N['rectangle'](s.get('rect',{}),viewport)
    else:require(g.get('notice') is None,'Unexpected native status')
    text=g.get('row',{}).get('text');require(isinstance(text,str) and all(v in text for v in O['received']()['text'].split('\n'))
        and 'Preserved original context' in text,'Original rich comment/context disappeared')
    return viewport


def changed_common(model,initial,journal,scope,reply,index,changed,previous):
    """Keep original successful-update oracle for wire/ACK and exact field deltas."""
    m=model['comments'];c=m['comments'];first=initial['comments']
    ack=O['fixture_ledger'](model['fixture'],index-3,changed,previous)
    for key in ('account','epoch','noSession','noTransport','writes','networkCalls','document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','composeLedger','reactionLedger','draftStore'):
        require(exact(m.get(key),initial.get(key)),f'Changed-format control altered independent owner/state: {key}')
    for key in ('active','record','root','loading','error','visibleTs','deleted','generation','hasMore','reactions'):
        require(exact(c.get(key),first.get(key)),f'Changed-format comment owner changed: {key}')
    expected_page=copy.deepcopy(first['page']);expected_typed=copy.deepcopy(first['pageMessages'])
    if index==8:expected_page['messages']=[copy.deepcopy(ack['message']) if v.get('ts')==O['TS'] else v for v in expected_page['messages']]
    if index>=7:
        for row in expected_typed:
            if row.get('ts')==O['TS']:
                for k in ('text','blocks','edited'):row[k]=copy.deepcopy(ack['message'][k])
    require(exact(c.get('page'),expected_page) and exact(c.get('pageMessages'),expected_typed),'Original HTTP/typed ACK/readback custody changed')
    ledger=m['ledger'];req=ledger['requests'];C['validate_reads'](req)
    require(len(req)==READS[index] and ledger.get('completed')==READS[index] and ledger.get('cancelled')==0 and ledger.get('held') is False
        and ledger.get('replies')==READS[index]//4 and [q['method'] for q in req]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(READS[index]//4)
        and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in req),'Changed-format readback acquired wrong owner/order')
    S['inventory'](journal,scope);S['paired'](m['draftStore']['drafts'][0],journal)
    return ack


def validate(fixture,receipt,directory,*,dimensions=None,png=True):
    require(receipt.get('case')==CASE and receipt.get('ok') is True and receipt.get('fixture')==fixture
        and fixture in C['FIXTURES'] and receipt.get('noLiveNetwork') is True,'Wrong run-equivalence receipt')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r.get('stage') for r in rows]==list(STAGES),'Missing/reordered rich-runs stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords']);require(len(receipt['initialRecords'])==73 and unrelated['count']==71,'Canonical records changed')
    initial=rows[0]['model']['comments'];seed=rows[0]['proof']['replySeed'];reply=rows[0]['proof']['replyBlocks'];journal=rows[0]['journal'];scope=journal['scope']
    expected_scope=dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))
    require(exact(scope,expected_scope) and exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']()) and seed.get('reopened') is True
        and R['integer'](seed.get('revision'),1) and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated)
        and initial['initialDocument'].get('id')==READ['LIST'],'Original independent reply seed/record changed')
    O['reply_content'](reply,seed['blocks']);disk_seed=T['RST']['seed_value'](rows[0]['model'])
    opened=[];old_actions=[];old_requests=[];old_fixture=None;history=None
    for i,row in enumerate(rows):
        p,m=row['proof'],row['model'];opens=p['opens']
        require(exact(p.get('source'),dict(received=O['received'](),boot=O['boot']())) and exact(p.get('replySeed'),seed) and exact(p.get('replyBlocks'),reply)
            and p.get('storageControls')==[] and exact(p.get('originalBlocks'),ORIGINAL if i else None),'Original source or rich seed changed')
        require(len(opens)==(0 if i==0 else 1 if i<5 else 2) and exact(opens[:len(opened)],opened),'Original Edit history changed')
        for op in opens:
            mount,a=op['mount'],op['active'];require(mount.get('ok') is True and isinstance(mount.get('mount'),str) and mount['mount']
                and exact(mount.get('blocks'),ORIGINAL) and mount.get('scope')==a.get('scope') and mount.get('generation')==str(a.get('generation'))
                and exact(a.get('source'),O['received']()) and exact(a.get('blocks'),ORIGINAL),'Original Edit opening/source changed')
            T['inventory'](op['inventory'],scope,disk_seed,ORIGINAL)
        require(len({v['mount']['mount'] for v in opens})==len(opens),'Fresh Edit reused old native mount');opened=opens
        on=p.get('formatting',{}).get('on',{}).get('native',{}).get('blocks')
        off=p.get('formatting',{}).get('off',{}).get('native',{}).get('blocks')
        changed=p.get('changed',{}).get('native',{}).get('blocks')
        blocks=ORIGINAL if i==1 else on if i==2 else off if i==3 else changed if i in (5,6) else None
        viewport=native(row['geometry'],i,reply,blocks)
        if i>=2:
            f=p['formatting'];mount=opens[0]['mount'];selection(f['selection'],mount);click(f['onClick'],mount,ORIGINAL,viewport)
            retained_italic(on);format_owner(f['on'],mount,on,'snapshot')
            require(all(exact(f.get(k),rows[2]['proof']['formatting'].get(k)) for k in ('selection','onClick','on')),'Original formatting capture history rewritten')
        if i>=3:
            click(f['offClick'],mount,on,viewport);format_owner(f['off'],mount,off,'snapshot');segmentation(off)
            # Pin the actual exercised boundary, not just a general normalizer.
            leaves=off[0]['elements'][0]['elements'];old=ORIGINAL[0]['elements'][0]['elements'][0]
            require(len(leaves)>1 and all(text_run(v) and exact(v.get('style'),old['style']) for v in leaves),'No actual ordinary first-section fragmentation')
            restored=copy.deepcopy(off);restored[0]['elements'][0]['elements']=[copy.deepcopy(old)]
            require(exact(restored,ORIGINAL) and ''.join(v['text'] for v in leaves)==old['text'],'Revert changed another section/container')
            require(exact(f,rows[3]['proof']['formatting']),'Original formatting-off history rewritten')
        if i>=4:
            before,after=p['saveBefore'],p['saveAfter'];P['no_updates'](before);P['no_updates'](after)
            require(exact(before,rows[3]['model']) and exact(after,rows[4]['model']) and exact(before['edit']['active']['blocks'],off)
                and exact(before['comments']['ledger'],after['comments']['ledger']) and exact(before['fixture']['policy'],after['fixture']['policy']),'Equivalent Save performed preflight or replaced original capture')
        if i>=5:
            mount=opens[1]['mount'];selection(p['changedSelection'],mount);click(p['changedClick'],mount,ORIGINAL,viewport)
            retained_italic(changed);format_owner(p['changed'],mount,changed,'snapshot')
            require(all(exact(p.get(k),rows[5]['proof'].get(k)) for k in ('changedSelection','changedClick','changed')),'Original changed-format capture history rewritten')
        if i<6:
            P['common'](m,initial,scope,reply,journal,READS[i],POLICIES[i]);P['active'](m,blocks,opens[-1]['mount'] if opens else None)
            T['inventory'](row['editInventory'],scope,disk_seed if blocks is not None else None,blocks or [])
            rn=row['rowNative'];require(rn.get('ok') is True and rn.get('ts')==O['TS'] and rn.get('editedMarkers')==0
                and rn.get('text')==row['geometry']['row']['text'],'No-op transcript acquired edited marker or foreign row')
        else:
            ack=changed_common(m,initial,journal,scope,reply,i,changed,old_fixture)
            e=m['edit'];attempts=e.get('attempts');require(e.get('opening') is False and e.get('events')==0 and e.get('retired')==[]
                and isinstance(attempts,list) and len(attempts)==1,'Changed-format Save duplicated/lost attempt')
            a=attempts[0];require(R['integer'](a.get('id'),1) and a.get('ts')==O['TS'] and exact(a.get('blocks'),changed+O['received']()['blocks'][1:])
                and a.get('phase')==('sent' if i==6 else 'confirmed') and exact(a.get('receipt'),None if i==6 else ack) and a.get('retryAt')==0,'Original changed attempt/ACK changed')
            if i==6:
                T['capture_state'](m,dict(editRemembered=opens[-1]['mount']),9,changed)
                T['inventory'](row['editInventory'],scope,disk_seed,changed,phase='Pending');history=row['editInventory']['attempt']
                require(history['capture']==row['editInventory']['capture'],'Pending changed save borrowed capture')
            else:
                P['active'](m,None);T['inventory'](row['editInventory'],scope,None,[])
                if i==8:require(exact(a,rows[7]['model']['edit']['attempts'][0]),'Confirmed attempt rewritten at readback')
            require(exact(p.get('doubleSave'),dict(ok=True,disabled=True)) and exact(p.get('release'),None if i==6 else O['RELEASE']),'Held changed Save/release proof changed')
        require(exact(row['journal'],journal),'Independent reply journal rewritten')
        req=m['comments']['ledger']['requests'];require(exact(req[:len(old_requests)],old_requests),'Read history rewritten');old_requests=req;old_fixture=m['fixture']
        ids=[] if i==0 else ['open']
        if i>=4:ids+=['slack-list-comment-edit-save']
        if i>=5:ids+=['open']
        if i>=6:ids+=['slack-list-comment-edit-save']
        if i>=8:ids+=['slack-list-comments-refresh']
        P['proof_actions'](p['actions'],ids,viewport);require(exact(p['actions'][:len(old_actions)],old_actions),'Action history rewritten');old_actions=p['actions']
        path=Path(directory)/f'{fixture}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign run-equivalence screenshot')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and exact(receipt['proof'],rows[-1]['proof']),'Final native proof/model changed')
    return receipt


def validate_closed(value,receipt):
    rows=receipt['stages'];last=rows[-1];changed=receipt['proof']['changed']['native']['blocks']
    require(isinstance(value,dict) and set(value)=={'account','inventories','editInventories','editHistory','contentSha256'}
        and value['account']==last['model']['comments']['account'] and exact(value['inventories'],[last['journal']])
        and exact(value['editInventories'],[last['editInventory']]) and len(value['editHistory'])==1,'Closed database lost original reply/clear or unique changed attempt')
    T['attempt'](value['editHistory'][0],T['RST']['seed_value'](rows[0]['model']),changed,'Confirmed',original=rows[6]['editInventory']['attempt'])
    require(isinstance(value['contentSha256'],str) and len(value['contentSha256'])==64 and all(c in '0123456789abcdef' for c in value['contentSha256']),'Missing closed SQL fingerprint')
