"""Authored byte-order and private-read custody mutations, not native execution."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import unittest
V=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_ordered.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit.py')))
D=Path('/private/ordered')
O=V['O']


def sample():
    f,base=B['sample']();rows=[];first=base['stages'][0];scope=first['journal']['scope'];journal=first['journal']
    p=copy.deepcopy(first['proof']);p['source']['received']=copy.deepcopy(V['INITIAL']['messages'][1]);p['storageControls']=[]
    initial=copy.deepcopy(first['model']['comments']);initial['comments']['page']=copy.deepcopy(V['INITIAL'])
    ds=V['disk_seed'](initial)
    def inv(i,blocks,active=False,attempt=None):
        return dict(scope=copy.deepcopy(scope),ts=O['TS'],revision=i,capture=f'00000000-0000-4000-8000-{i:012d}' if i else '',
            seed=copy.deepcopy(ds) if active else None,blocks=copy.deepcopy(blocks),attempt=copy.deepcopy(attempt),retry_at=0)
    original=dict(path='/private/nonce/comments.sqlite',account=initial['account'],scope=scope,reply=journal,edit=inv(0,[]));p['originalDisk']=copy.deepcopy(original)
    mount=dict(ok=True,scope='list-edit:100:77',generation='77',mount='original-native-mount',blocks=copy.deepcopy(V['ORIGINAL']))
    pending=dict(id='00000000-0000-4000-8000-000000000080',capture='00000000-0000-4000-8000-000000000002',phase='Pending',raw=None,readback=None,retry_at=0,
        record=dict(version=1,seed=copy.deepcopy(ds),blocks=copy.deepcopy(V['FUTURE']['messages'][1]['blocks']),text=V['FUTURE']['messages'][1]['text']))
    requests=[];pages=[]
    for i,name in enumerate(V['STAGES']):
        source=(0,1,3,4,5,5,5)[i];row=copy.deepcopy(base['stages'][source]);row.update(stage=name,capture=str(D/f'{f}-{name}.png'))
        model=row['model'];m=model['comments'];c=m['comments'];m['draftStore']=copy.deepcopy(initial['draftStore'])
        c.update(generation=100,hasMore=False,loading=i==5,page=copy.deepcopy(V['INITIAL'] if i<4 else V['FUTURE']),pageMessages=copy.deepcopy((V['INITIAL'] if i<3 else V['FUTURE'])['messages']))
        n=V['READS'][i];cycle=first['model']['comments']['ledger']['requests'][:4]
        m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(cycle*(n//4))],completed=n-int(i==5),cancelled=int(i==6),held=False,replies=n//4)
        model['fixture'].update(received=copy.deepcopy(V['INITIAL']['messages'][1]),policy=[dict(request=q,response=r) for _ in range(V['POLICIES'][i]) for q,r in O['policy_cycle']()],published=False,publishedRow=None)
        ff=model['fixture'];count=i>=2;wire=O['wire'](V['CHANGED'])
        ff.update(prepared=[wire] if count else [],updates=[copy.deepcopy(wire)] if count else [],responses=[dict(disposition='accepted',raw=copy.deepcopy(V['ACK']))] if count else [],
            futureRows=[copy.deepcopy(V['FUTURE']['messages'][1])] if count else [],held=i==2,releases=[O['RELEASE']] if i>=3 else [])
        e=model['edit'];e['retired']=[];blocks=V['ORIGINAL'] if i==1 else V['CHANGED'] if i==2 else None
        if blocks is not None:
            a=e['active'];a.update(blocks=copy.deepcopy(blocks),source=copy.deepcopy(V['INITIAL']['messages'][1]),preserved=copy.deepcopy(V['INITIAL']['messages'][1]['blocks'][1:]),nativeReady=True,captureFailed=False)
            model['capture']=dict(scope=mount['scope'],generation=77,mount=mount['mount'],failed=False,canSave=i==1,canCancel=i==1,canCheck=False,canRetry=False,busy=i==2,finishing=False)
        else:e['active']=None;model['capture']=None
        e['attempts']=[dict(id=123,ts=O['TS'],blocks=copy.deepcopy(V['FUTURE']['messages'][1]['blocks']),phase='sent' if i==2 else 'confirmed',receipt=None if i==2 else copy.deepcopy(V['ACK']),retryAt=0)] if count else []
        row['editInventory']=inv(i,blocks or [],blocks is not None,pending if i==2 else None)
        g=row['geometry'];g['row']['text']=(V['INITIAL'] if i<3 else V['FUTURE'])['messages'][1]['text']+' Preserved original context'
        if blocks is not None:g['edit'].update(blocks=copy.deepcopy(blocks),sameNode=True)
        if i==1:
            p['open']=dict(ok=True,proof=dict(disabled=False,visible=True,hit=True,rect=B['B']['rect']()));p['mount']=mount
        if i==2:
            p.update(append=dict(ok=True,blocks=copy.deepcopy(V['CHANGED'])),editedBlocks=copy.deepcopy(V['CHANGED']),doubleSave=dict(ok=True,disabled=True))
        if i==3:p['release']=O['RELEASE']
        if i in (2,4,5):
            id='slack-list-comment-edit-save' if i==2 else 'slack-list-comments-refresh'
            p['actions'].append(dict(id=id,native=dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=B['B']['rect']()))))
        while len(requests)<n//4:
            j=len(requests);text=V['INITIAL_BYTES'] if j<3 else V['FUTURE_BYTES'];raw=V['decoded'](text)
            requests.append(dict(serial=(j+1)*4,readSerial=j+1,method='conversations.replies',args=copy.deepcopy(cycle[3]['args']),variant='initial' if j<3 else 'future',bytes=text,raw=raw,owner=dict(serial=100,account=initial['account'],epoch=initial['epoch'])))
            if j<4:pages.append(dict(raw=copy.deepcopy(raw),messages=copy.deepcopy(raw['messages']),witness=dict(messageIndex=1,raw=copy.deepcopy(raw['messages'][1]),styleKeys=['italic','bold']),requestSerial=(j+1)*4,readSerial=j+1,current=True))
        entry=dict(raw=copy.deepcopy(c['page']),messages=copy.deepcopy(c['pageMessages']),witness=dict(messageIndex=1,raw=copy.deepcopy(c['page']['messages'][1]),styleKeys=['italic','bold']))
        row['ordered']=dict(initialBytes=V['INITIAL_BYTES'],futureBytes=V['FUTURE_BYTES'],requests=copy.deepcopy(requests),pages=copy.deepcopy(pages),entryPage=entry,
            releases=[],held=i==5,completed=(1,2,3,3,4,4,5)[i],cancelled=int(i==6),heldOwnerCurrent=True if i==5 else False if i==6 else None,plainModeEnabled=False)
        if i==5:p['beforeReplacementDisk']=dict(copy.deepcopy(original),edit=copy.deepcopy(row['editInventory']))
        if i==6:
            row['editInventory']=copy.deepcopy(rows[5]['editInventory'])
            p['afterReplacementDisk']=copy.deepcopy(p['beforeReplacementDisk']);p['replacement']=dict(originalAccount=initial['account'],originalEpoch=initial['epoch'],syntheticAccountReplacement=True,release=dict(cancelledBeforeReply=True,delivered=False),detached=dict(ok=True))
            m.update(account='list-ordered-successor',epoch=initial['epoch']+1,noSession=True,noTransport=True,comments=None,document=None,records=None,unrelated=None,detail=None,draftStore=dict(active=None,drafts=[]))
            row['model']=m;row['geometry']=dict(ok=True);row['ordered'].update(entryPage=None,releases=[p['replacement']['release']])
        row['proof']=copy.deepcopy(p);rows.append(row)
    r=dict(ok=True,case=V['CASE'],fixture=f,readiness=base['readiness'],stages=rows,initialRecords=base['initialRecords'],noLiveNetwork=True,proof=copy.deepcopy(p),final=copy.deepcopy(rows[-1]['model']))
    confirmed=dict(copy.deepcopy(pending),phase='Confirmed',raw=copy.deepcopy(V['ACK']))
    closed=dict(account=initial['account'],inventories=[journal],editInventories=[rows[-1]['editInventory']],editHistory=[confirmed],contentSha256='a'*64)
    return f,r,closed


class OrderedTests(unittest.TestCase):
    def reject(self,change):
        f,r,_=sample();change(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):V['validate'](f,r,D,png=False)

    def test_full_seven_stages_and_closed_original_sqlite(self):
        f,r,c=sample();V['validate'](f,r,D,png=False);V['validate_closed'](c,r)

    def test_authored_hashes_not_reserialized_json(self):
        for key,sha,size in [('INITIAL_BYTES','a05ec755158bec9742e436d565712b4ff035fcd2929cf4b92009a22f17f27a16',987),('FUTURE_BYTES','7b7d0f9cb5e17c2c54c933a2ee6f79ae9ecc82f81e990e8da1f36c7a17b94dad',1060)]:
            raw=V[key].encode();self.assertEqual(len(raw),size);self.assertEqual(hashlib.sha256(raw).hexdigest(),sha)
        self.reject(lambda r:r['stages'][0]['ordered'].update(initialBytes=json.dumps(V['INITIAL'])))

    def test_duplicate_json_keys_refused(self):
        with self.assertRaises(RuntimeError):V['decoded']('{"ok":true,"messages":[],"ok":true}')

    def test_style_order_cannot_be_sorted_or_borrowed(self):
        self.reject(lambda r:r['stages'][2]['ordered']['pages'][2]['witness'].update(styleKeys=['bold','italic']))
        self.reject(lambda r:r['stages'][0]['ordered']['pages'][0]['witness'].update(messageIndex=0))

    def test_actual_private_preflight_callback_required(self):
        self.reject(lambda r:r['stages'][2]['ordered']['pages'].pop())
        self.reject(lambda r:r['stages'][2]['ordered']['pages'][2].update(readSerial=1))
        self.reject(lambda r:r['stages'][2]['ordered']['pages'][2].update(current=False))

    def test_typed_ack_cannot_rewrite_ordered_raw(self):
        self.reject(lambda r:r['stages'][3]['ordered']['entryPage'].update(raw=copy.deepcopy(V['FUTURE'])))
        self.reject(lambda r:r['stages'][3]['ordered']['entryPage']['witness'].update(raw=copy.deepcopy(V['FUTURE']['messages'][1])))

    def test_ack_must_update_typed_page(self):
        self.reject(lambda r:r['stages'][3]['ordered']['entryPage'].update(messages=copy.deepcopy(V['INITIAL']['messages'])))

    def test_future_readback_is_independently_authored(self):
        self.reject(lambda r:r['stages'][4]['ordered']['requests'][3].update(bytes=V['INITIAL_BYTES']))
        self.reject(lambda r:r['stages'][4]['ordered']['pages'][3]['raw']['messages'][1].update(text='rewritten'))

    def test_foreign_request_owner_refused(self):
        self.reject(lambda r:r['stages'][1]['ordered']['requests'][1]['owner'].update(account='foreign'))
        self.reject(lambda r:r['stages'][1]['ordered']['requests'][1]['owner'].update(epoch=99))

    def test_held_response_never_adopted_by_successor(self):
        self.reject(lambda r:r['stages'][6]['ordered']['pages'].append(copy.deepcopy(r['stages'][6]['ordered']['pages'][-1])))
        self.reject(lambda r:r['stages'][6]['ordered'].update(heldOwnerCurrent=True))
        self.reject(lambda r:r['stages'][6]['proof']['replacement']['release'].update(delivered=True))
        self.reject(lambda r:r['stages'][6]['model'].update(comments=copy.deepcopy(r['stages'][5]['model']['comments']['comments'])))

    def test_no_duplicate_update_or_lost_reply(self):
        self.reject(lambda r:r['stages'][2]['model']['fixture']['updates'].append({}))
        self.reject(lambda r:r['stages'][5]['journal'].update(blocks=[]))
        self.reject(lambda r:r['stages'][6]['editInventory'].update(blocks=copy.deepcopy(V['CHANGED'])))

    def test_original_native_control_and_capture_required(self):
        self.reject(lambda r:r['stages'][2]['geometry']['edit'].update(sameNode=False))
        self.reject(lambda r:r['stages'][1]['model']['capture'].update(finishing=True))
        self.reject(lambda r:r['stages'][1]['proof']['open']['proof'].update(hit=False))

    def test_closed_history_owner_and_exact_wire(self):
        f,r,c=sample()
        for change in (lambda x:x.update(account='list-ordered-successor'),lambda x:x['editHistory'].append(copy.deepcopy(x['editHistory'][0])),lambda x:x['editHistory'][0]['record'].update(blocks=[])):
            v=copy.deepcopy(c);change(v)
            with self.assertRaises(RuntimeError):V['validate_closed'](v,r)

    def test_wrapper_preserves_original_guards(self):
        w=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-ordered-native.py')));h=w['harness']()
        self.assertEqual(h['case_contract']('own-edit-ordered')[0],V['STAGES'])
        with self.assertRaises(RuntimeError):w['storage_controller']('wrong')
        code=h['run_fixture'].__code__.co_names
        for name in ('NativeFixture','validate_log','require_running','validate_closed','cleanup_errors','file_digest','closed_edit_snapshot'):self.assertIn(name,code)

if __name__=='__main__':unittest.main()
