"""Original ordered HTTP reply bytes remain distinct from typed comment updates."""
import copy
import hashlib
import json
from pathlib import Path
import runpy

BASE=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit.py')))
require,exact=BASE['require'],BASE['exact']
CASE='own-edit-ordered'
STAGES=('draft','editing','saving','saved','readback','held','replaced')


def decoded(text):
    """Independent strict decode keeps observed object order from original text."""
    require(isinstance(text,str) and len(text.encode('utf-8'))<=1024*1024,'Missing/oversized original reply bytes')
    def pairs(values):
        out={}
        for key,value in values:
            require(key not in out,'Duplicate original reply key')
            out[key]=value
        return out
    value=json.loads(text,object_pairs_hook=pairs,parse_constant=lambda _:(_ for _ in ()).throw(RuntimeError('Nonfinite reply value')))
    require(isinstance(value,dict) and value.get('ok') is True and value.get('error') is None
        and isinstance(value.get('messages'),list),'Original reply response was not successful')
    return value


def page_witness(observed,text,expected_message_index=1):
    """Use the exact replies array position; never recover order from a Value."""
    response=decoded(text)
    require(0<=expected_message_index<len(response['messages']),'Original message index unavailable')
    message=response['messages'][expected_message_index]
    require(isinstance(observed,dict) and exact(observed.get('raw'),response),'Parsed Page lost original raw response')
    witness=observed.get('witness')
    require(isinstance(witness,dict) and type(witness.get('messageIndex')) is int and witness['messageIndex']==expected_message_index
        and exact(witness.get('raw'),message),'Ordered witness borrowed another raw message/index')
    keys=list(message['blocks'][0]['elements'][0]['elements'][0]['style'])
    require(keys==['italic','bold'] and exact(witness.get('styleKeys'),keys),'Original nonalphabetic style order was reconstructed or changed')
    return response,message

# Authored fixture byte literals pinned independently at oracle construction.
INITIAL_BYTES='{"ok":true,"channel":"CCOMMENT01","messages":[{"ts":"1781524800.000001","subtype":"list_record_comment","slack_list":{"list_id":"FCOMMENT01","list_record_id":"R_LIST_000"},"text":"Record comment root","unknown":{"keep":true}},{"type":"message","ts":"1781524801.000001","thread_ts":"1781524800.000001","user":"ULIST0001","text":"Original own comment.\\nSecond editable line.","edit_fixture_opaque":{"keep":"original receipt"},"reactions":[{"name":"wave","count":1,"users":["ULIST0002"]}],"blocks":[{"type":"rich_text","block_id":"own-edit-rich","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original own comment.","style":{"italic":true,"bold":true}}]},{"type":"rich_text_section","elements":[{"type":"text","text":"Second editable line."}]}]},{"type":"context","block_id":"own-edit-context","elements":[{"type":"plain_text","text":"Preserved original context"}],"edit_context_opaque":{"keep":true}}]}],"has_more":false,"response_metadata":{"next_cursor":""}}\n'
FUTURE_BYTES='{"ok":true,"channel":"CCOMMENT01","messages":[{"ts":"1781524800.000001","subtype":"list_record_comment","slack_list":{"list_id":"FCOMMENT01","list_record_id":"R_LIST_000"},"text":"Record comment root","unknown":{"keep":true}},{"type":"message","ts":"1781524801.000001","thread_ts":"1781524800.000001","user":"ULIST0001","text":"Original own comment.\\nSecond editable line. Changed.","edit_fixture_opaque":{"keep":"original receipt"},"reactions":[{"name":"wave","count":1,"users":["ULIST0002"]}],"blocks":[{"type":"rich_text","block_id":"own-edit-rich","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original own comment.","style":{"italic":true,"bold":true}}]},{"type":"rich_text_section","elements":[{"type":"text","text":"Second editable line. Changed."}]}]},{"type":"context","block_id":"own-edit-context","elements":[{"type":"plain_text","text":"Preserved original context"}],"edit_context_opaque":{"keep":true}}],"edited":{"user":"ULIST0001","ts":"1781525800.000001"}}],"has_more":false,"response_metadata":{"next_cursor":""}}\n'
INITIAL=decoded(INITIAL_BYTES)
FUTURE=decoded(FUTURE_BYTES)
ORIGINAL=copy.deepcopy(INITIAL['messages'][1]['blocks'][:1])
CHANGED=copy.deepcopy(FUTURE['messages'][1]['blocks'][:1])
ACK=dict(ok=True,channel='CCOMMENT01',ts='1781524801.000001',message=FUTURE['messages'][1])
O=BASE
# Isolated module dictionaries only: original baseline scripts remain byte-exact.
O['received'].__globals__['received']=lambda:copy.deepcopy(INITIAL['messages'][1])
O['received']=O['received'].__globals__['received']
O['geometry'].__globals__['CHANGE']=' Changed.'
T=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_edit_storage.py')))
T['attempt'].__globals__['O']=O
P=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_noop.py')))
P['active'].__globals__['O']=O
N,G,R,C,READ,H,S=(O[k] for k in ('N','G','R','C','READ','H','S'))
READS=(4,8,12,12,16,20,20)
PAGES=(1,2,3,3,4,4,4)
POLICIES=(0,1,2,2,2,2,2)


def typed(messages,raw):
    require(isinstance(messages,list) and len(messages)==2,'Parsed Page messages omitted/duplicated')
    for observed,source in zip(messages,raw['messages']):
        require(all(exact(observed.get(k),v) for k,v in source.items()) and exact(observed.get('edited'),source.get('edited')),'Typed Page changed authored message content')


def ordered(value,index,owner,ledger,previous=None):
    require(value.get('initialBytes')==INITIAL_BYTES and value.get('futureBytes')==FUTURE_BYTES
        and value.get('plainModeEnabled') is False,'Authored HTTP byte text changed or plain mode enabled')
    req,pages=value['requests'],value['pages']
    require(len(req)==READS[index]//4 and len(pages)==PAGES[index]
        and value.get('completed')==(1,2,3,3,4,4,5)[index]
        and value.get('cancelled')==int(index==6) and value.get('held') is (index==5)
        and exact(value.get('heldOwnerCurrent'),True if index==5 else False if index==6 else None)
        and exact(value.get('releases'),[dict(cancelledBeforeReply=True,delivered=False)] if index==6 else []),
        'Original ordered read/parse/disposition count changed')
    for j,q in enumerate(req):
        text=INITIAL_BYTES if j<3 else FUTURE_BYTES
        generic=ledger['requests'][(j+1)*4-1]
        require(q.get('serial')==(j+1)*4 and q.get('method')=='conversations.replies'
            and exact(q.get('args'),generic['args']) and q.get('variant')==('initial' if j<3 else 'future')
            and q.get('bytes')==text and exact(q.get('raw'),decoded(text))
            and q.get('owner',{}).get('account')==owner['account'] and q['owner'].get('epoch')==owner['epoch']
            and R['integer'](q['owner'].get('serial'),1) and R['integer'](q.get('readSerial'),1),
            'Ordered request borrowed owner/factory or reconstructed original bytes')
        if j<len(pages):
            p=pages[j];page_witness(p,text);typed(p['messages'],decoded(text))
            require(p.get('current') is True and p.get('requestSerial')==q['serial'] and p.get('readSerial')==q['readSerial'],
                'Actual parser observation lost original private read identity')
    if previous:
        for key in ('requests','pages','releases'):
            require(exact(value[key][:len(previous[key])],previous[key]),'Original ordered history rewritten')
    entry=value.get('entryPage')
    if index==6:require(entry is None,'Retired Page adopted by successor');return
    page_witness(entry,INITIAL_BYTES if index<4 else FUTURE_BYTES)
    typed(entry['messages'],FUTURE if index>=3 else INITIAL)


def fixture(value,index):
    require(value.get('enabled') is True and value.get('networkCalls')==0 and value.get('httpCapability') is False
        and exact(value.get('received'),INITIAL['messages'][1]) and exact(value.get('boot'),O['boot']()),'Original fixture authority changed')
    require(exact(value.get('policy'),[dict(request=q,response=r) for _ in range(POLICIES[index]) for q,r in O['policy_cycle']()]),'Private policy/source acquisition changed')
    count=int(index>=2)
    require(len(value['prepared'])==count and exact(value['updates'],value['prepared']),'Wrong number of actual updates')
    for request in value['prepared']:O['update_wire'](request,CHANGED)
    require(exact(value['responses'],[dict(disposition='accepted',raw=ACK)] if count else [])
        and exact(value['futureRows'],[FUTURE['messages'][1]] if count else [])
        and value.get('held') is (index==2) and exact(value.get('releases'),[O['RELEASE']] if index>=3 else [])
        and value.get('published') is False and value.get('publishedRow') is None,'Update ACK or ordered-only future publication changed')


def disk_seed(initial):
    return dict(workspace='T1',user=O['SELF'],ts=O['TS'],target=dict(document=initial['initialDocument'],
        record=initial['records'][READ['KNOWN']],channel=O['CHANNEL'],root=READ['ROOT_TS']),
        http=INITIAL,editable=ORIGINAL,preserved=INITIAL['messages'][1]['blocks'][1:])


def ledger(value,index):
    requests=value['requests'];C['validate_reads'](requests)
    require(len(requests)==READS[index] and value.get('completed')==READS[index]-int(index==5)
        and value.get('cancelled')==int(index==6) and value.get('held') is False and value.get('replies')==READS[index]//4
        and [q['method'] for q in requests]==['files.info','lists.records.info','experiments.getByUser','conversations.replies']*(READS[index]//4)
        and all(q['args'].get('id',READ['KNOWN'])==READ['KNOWN'] and (q['method']!='conversations.replies' or q['args']['ts']==READ['ROOT_TS']) for q in requests),
        'Original ordinary/private read order or cancellation changed')


def disk(value,original,journal,inventory):
    require(isinstance(value,dict) and set(value)=={'path','account','scope','reply','edit'}
        and all(exact(value.get(k),original.get(k)) for k in ('path','account','scope'))
        and exact(value.get('reply'),journal) and exact(value.get('edit'),inventory),'Original private database scope/reply/edit custody changed')


def validate(fixture_name,receipt,directory,*,dimensions=None,png=True):
    require(receipt.get('case')==CASE and receipt.get('ok') is True and receipt.get('fixture')==fixture_name
        and fixture_name in C['FIXTURES'] and receipt.get('noLiveNetwork') is True,'Wrong original ordered-native terminal')
    ready=receipt['readiness'];H['validate_ready'](ready,C['ready_tuple'](fixture_name,ready.get('nonce'),ready.get('pid')))
    rows=receipt['stages'];require([r.get('stage') for r in rows]==list(STAGES),'Missing/reordered original ordered stages')
    touched,unrelated=READ['record_proof'](receipt['initialRecords'])
    initial=rows[0]['model']['comments'];first=initial['comments'];p0=rows[0]['proof'];reply=p0['replyBlocks'];seed=p0['replySeed'];journal=rows[0]['journal'];scope=journal['scope']
    require(len(receipt['initialRecords'])==73 and unrelated['count']==71 and exact(initial['records'],touched) and exact(initial['unrelated'],unrelated)
        and initial['initialDocument'].get('id')==READ['LIST'] and exact(initial['document'],initial['initialDocument'])
        and R['uuid'](initial.get('account')) and exact(scope,dict(workspace='T1',user=O['SELF'],list=READ['LIST'],record=READ['KNOWN'],root=READ['ROOT_TS'],
            created=touched[READ['KNOWN']].get('date_created'),creator=touched[READ['KNOWN']].get('created_by'))),'Original canonical List/owner changed')
    require(exact(seed.get('scope'),scope) and exact(seed.get('blocks'),G['seed_blocks']()) and seed.get('reopened') is True and R['integer'](seed.get('revision'),1),'Original reply seed changed')
    O['reply_content'](reply,seed['blocks']);S['inventory'](journal,scope)
    original=p0['originalDisk'];require(original['account']==initial['account'] and exact(original['scope'],scope) and isinstance(original['path'],str) and original['path'].endswith('.sqlite'),'Original private database address missing')
    ds=disk_seed(initial);old_order=None;old_req=[];old_proof={};old_actions=[]
    for i,row in enumerate(rows):
        p=row['proof'];m=row['model'] if i==6 else row['model']['comments']
        require(exact(p.get('source'),dict(received=INITIAL['messages'][1],boot=O['boot']())) and exact(p.get('replySeed'),seed)
            and exact(p.get('replyBlocks'),reply) and exact(p.get('originalDisk'),original) and p.get('storageControls')==[], 'Original source/proof replaced')
        require(all(exact(p.get(k),v) for k,v in old_proof.items() if k!='actions'),'Original action/capture history rewritten');old_proof=copy.deepcopy(p)
        require(exact(p['actions'][:len(old_actions)],old_actions),'Original native action history rewritten')
        if i==6:require(exact(p['actions'],old_actions),'Replacement performed an unexpected native action')
        old_actions=copy.deepcopy(p['actions'])
        require(exact(row['journal'],journal),'Independent reply journal rewritten')
        ledger(m['ledger'],i);require(exact(m['ledger']['requests'][:len(old_req)],old_req),'Original HTTP ledger rewritten');old_req=m['ledger']['requests']
        ordered(row['ordered'],i,initial,m['ledger'],old_order);old_order=row['ordered']
        require(m.get('writes')==0 and m.get('networkCalls')==0 and exact(m.get('reactionLedger'),initial['reactionLedger'])
            and exact(m.get('composeLedger'),initial['composeLedger']),'Ordered exercise dispatched unrelated mutation')
        if i==6:
            require(m.get('account')=='list-ordered-successor' and m.get('epoch')==initial['epoch']+1 and m.get('noSession') is True and m.get('noTransport') is True
                and all(m.get(k) is None for k in ('comments','document','records','unrelated','detail')) and row['geometry']=={'ok':True},'Synthetic successor retained old owner/UI')
            rep=p['replacement'];require(rep.get('syntheticAccountReplacement') is True and rep.get('originalAccount')==initial['account'] and rep.get('originalEpoch')==initial['epoch']
                and exact(rep.get('release'),dict(cancelledBeforeReply=True,delivered=False)) and exact(rep.get('detached'),row['geometry']),'Original held response was not retired before delivery')
            disk(p['afterReplacementDisk'],original,journal,row['editInventory']);require(exact(p['afterReplacementDisk'],p['beforeReplacementDisk']),'Replacement changed original durable journals')
        else:
            require(m.get('account')==initial['account'] and m.get('epoch')==initial['epoch'] and m.get('noSession') is False and m.get('noTransport') is False,'Original account/session changed')
            for key in ('document','initialDocument','records','initialRecords','unrelated','initialUnrelated','detail','draftStore'):
                require(exact(m.get(key),initial.get(key)),f'Original independent state changed: {key}')
            c=m['comments'];require(c.get('active') is True and c.get('record')==READ['KNOWN'] and c.get('root')==READ['ROOT_TS']
                and c.get('loading') is (i==5) and c.get('error') is None and c.get('visibleTs')==[O['TS']]
                and c.get('generation')==first['generation'] and c.get('deleted')==[] and c.get('hasMore') is False,'Original comment owner/readiness changed')
            require(exact(c.get('page'),row['ordered']['entryPage']['raw']) and exact(c.get('pageMessages'),row['ordered']['entryPage']['messages']),'Installed Page and actual ordered observation diverged')
            S['paired'](m['draftStore']['drafts'][0],journal)
            model=row['model'];fixture(model['fixture'],i)
            blocks=ORIGINAL if i==1 else CHANGED if i==2 else None
            viewport=O['geometry'](row['geometry'],(0,1,3,4,5,5)[i],reply,ORIGINAL,CHANGED)
            if i in (1,2):require(row['geometry']['edit'].get('sameNode') is True,'Original edit node replaced')
            if i==1:P['active'](model,ORIGINAL,p['mount'])
            elif i==2:T['capture_state'](model,dict(editRemembered=p['mount']),9,CHANGED)
            else:P['active'](model,None)
            e=model['edit'];require(len(e['attempts'])==int(i>=2),'Missing/duplicate update attempt')
            if i>=2:
                a=e['attempts'][0];require(R['integer'](a.get('id'),1) and a.get('ts')==O['TS'] and exact(a.get('blocks'),FUTURE['messages'][1]['blocks'])
                    and a.get('phase')==('sent' if i==2 else 'confirmed') and exact(a.get('receipt'),None if i==2 else ACK) and a.get('retryAt')==0,'Changed update/ACK lost exact source body')
                require(p.get('append',{}).get('ok') is True and exact(p['append'].get('blocks'),CHANGED) and exact(p.get('editedBlocks'),CHANGED)
                    and exact(p.get('doubleSave'),dict(ok=True,disabled=True)) and exact(p.get('release'),None if i==2 else O['RELEASE']),'Actual native append/held Save evidence changed')
            T['inventory'](row['editInventory'],scope,ds if blocks is not None else None,blocks or [],phase='Pending' if i==2 else None)
            if i>=1:
                mount=p['mount'];require(mount.get('ok') is True and isinstance(mount.get('mount'),str) and bool(mount['mount']) and exact(mount.get('blocks'),ORIGINAL),'Original native Edit mount changed')
                P['proof_actions']([p['open']],['open'],viewport)
            ids=[] if i<2 else ['slack-list-comment-edit-save']+['slack-list-comments-refresh']*max(0,i-3)
            P['proof_actions'](p['actions'],ids,viewport)
        if i==0:disk(original,original,journal,row['editInventory'])
        if i==5:disk(p['beforeReplacementDisk'],original,journal,row['editInventory'])
        if i==6:T['inventory'](row['editInventory'],scope,None,[])
        path=Path(directory)/f'{fixture_name}-{row["stage"]}.png';require(Path(row.get('capture',''))==path,'Foreign ordered screenshot')
        if png:H['png_signature'](path,dimensions)
    require(exact(receipt['final'],rows[-1]['model']) and exact(receipt['proof'],rows[-1]['proof']),'Terminal successor/proof rewritten')
    return receipt


def validate_closed(value,receipt):
    rows=receipt['stages'];last=rows[-1];original=receipt['proof']['originalDisk']
    require(isinstance(value,dict) and set(value)=={'account','inventories','editInventories','editHistory','contentSha256'}
        and value['account']==original['account'] and exact(value['inventories'],[last['journal']])
        and exact(value['editInventories'],[last['editInventory']]) and len(value['editHistory'])==1,'Closed original database lost reply/clear/unique update')
    T['attempt'](value['editHistory'][0],disk_seed(rows[0]['model']['comments']),CHANGED,'Confirmed',original=rows[2]['editInventory']['attempt'])
    require(isinstance(value['contentSha256'],str) and len(value['contentSha256'])==64 and all(c in '0123456789abcdef' for c in value['contentSha256']),'Missing closed SQLite fingerprint')
