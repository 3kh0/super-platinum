"""Portable deletion receipts and adversarial request, journal and native checks."""
import copy
import runpy
import unittest
from pathlib import Path

V=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_own_delete.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_own_edit.py')))
DIRECTORY=Path('/private/own-delete')


def sample(case='own-delete',narrow=False):
    fixture,base=B['sample'](narrow)
    first=base['stages'][0]
    initial=first['model']['comments']
    record=copy.deepcopy(V['source'](initial));reply=first['proof']['replyBlocks'];seed=first['proof']['replySeed']
    readset=initial['ledger']['requests'][:4]
    rows=[]
    for i,name in enumerate(V['STAGES'][case]):
        modal,held,removed,published,reopened,count=V['layout'](case,i)
        m=copy.deepcopy(initial);c=m['comments'];n=V['READS'][case][i]
        m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,replies=n//4,cancelled=0,held=False)
        c.update(generation=200 if reopened else 100,loading=False,error=None,retryAt=0,
            deleted=[V['TS']] if removed and not reopened else [],visibleTs=[V['READ']['ROOT_TS']] if removed else [V['TS']],
            accepted=[],live=[],acceptedTs=[],liveTs=[])
        if removed:
            c['pageMessages']=[r for r in c['pageMessages'] if r.get('ts')!=V['TS']]
            c['reactions']=[dict(ts=V['READ']['ROOT_TS'],reactions=[])]
        if (case=='own-delete' and i>=5) or reopened:c['page']=V['canonical'](initial)
        draft=m['draftStore']['drafts'][0]
        draft.update(id='restored' if reopened else 'original',generation=40 if reopened else 20)
        m['draftStore']['active']=f"{draft['id']}:{c['generation']}:{draft['generation']}"
        f=dict(enabled=True,seed=dict(received=V['O']['received'](),boot=V['O']['boot']()),received=V['O']['received'](),
            policy=V['policy_cycle']()*V['POLICIES'][case][i],prepared=[V['wire']() for _ in range(count)],
            deletes=[V['wire']() for _ in range(count)],records=[copy.deepcopy(record) for _ in range(count)],
            held=held,releases=[V['O']['RELEASE']] if case=='own-delete' and i>=4 else [],
            published=published,deleted=[V['TS']] if published else [],networkCalls=0,httpCapability=False)
        if case=='own-delete':
            f['responses']=[dict(disposition='accepted',raw=copy.deepcopy(V['ACK']),retryAfter=None)] if count else []
            f['futureDeleted']=V['TS'] if count else None
        else:
            f['responses']=[dict(disposition='rejected',raw=copy.deepcopy(V['REJECTED']),retryAfter=None),dict(disposition='unconfirmed',raw=None,retryAfter=None)][:count]
            f['futureDeleted']=V['TS'] if count==2 else None
        active=None
        if modal:
            phase=None if case=='own-delete' else V['PHASES'][case][i]
            active=dict(id=12 if case!='own-delete' or i==1 else 14,generation=100,ts=V['TS'],busy=held,phase=phase,
                localPending=False,sourceCurrent=True,record=copy.deepcopy(record),canConfirm=not held and phase!='Uncertain',
                canCheck=phase=='Uncertain',canRetryLocal=False)
        delete=dict(active=active,retainedCount=int(i>0))
        inv=None
        if i>0:
            phase=V['PHASES'][case][i]
            a=None
            if phase:
                aid='00000000-0000-4000-8000-000000000002' if case=='own-delete-failure' and i>=2 else '00000000-0000-4000-8000-000000000001'
                a=dict(id=aid,record=copy.deepcopy(record),phase=phase,
                    raw=copy.deepcopy(V['ACK']) if phase=='Confirmed' else copy.deepcopy(V['REJECTED']) if phase=='Rejected' else None,
                    readback=dict(target=record['target'],raw=V['canonical'](initial)) if phase=='Observed' else None,retry_at=0)
            revision=((0,0,0,1,2,2,2) if case=='own-delete' else (0,2,4,4,5,5))[i]
            inv=dict(scope=seed['scope'],ts=V['TS'],revision=revision,attempt=a,retry_at=0)
        p=dict(source=dict(received=V['O']['received'](),boot=V['O']['boot']()),replySeed=copy.deepcopy(seed),replyBlocks=copy.deepcopy(reply))
        if i>=(3 if case=='own-delete' else 2):p['blockedConfirm']=copy.deepcopy(V['BLOCKED'])
        if case=='own-delete' and i>=4:p['release']=copy.deepcopy(V['O']['RELEASE'])
        if reopened:p['runtimeForgotten']=True
        rect=B['B']['rect'];names=([V['PREFIX']+'check'] if case=='own-delete-failure' and i in (2,3) else [])+[V['PREFIX']+'cancel',V['PREFIX']+'confirm'] if modal else ['slack-list-comments-back','slack-list-comment-send']
        controls=[dict(id=name,text=name,type='button',visible=True,hit=True,rect=rect(),
            disabled=held or (name==V['PREFIX']+'confirm' and case=='own-delete-failure' and i in (2,3))) for name in names]
        panel=None
        if modal:
            panel=dict(id='slack-list-comment-delete',role='alertdialog',modal='true',busy=str(held).lower(),generation='100',session=str(active['id']),
                title='Delete message',description='Are you sure you want to delete this message? This cannot be undone.',visible=True,rect=rect(),
                preview=dict(text=V['O']['received']()['text'],body='Preserved original context',bold=['Original own comment \u4e2d\U0001f642.'],
                    italic=['Second editable line.'],visible=True,inert=True,actions=0,tools=0,rect=rect()))
        notice='Working\u2026' if held else V['NOTICES'].get(name)
        g=dict(ok=True,noOverflow=True,viewport=[480,800] if narrow else [1100,800],controls=controls,
            text=None if modal else B['B']['textproof'](),panel=panel,
            notice=dict(text=notice,role='status',visible=True,hit=True,rect=rect()) if notice else None,
            reply=dict(blocks=copy.deepcopy(reply),generation=str(draft['generation']),composing=False,sameNode=not reopened,visible=not modal,hidden=modal,inert=modal,rect=rect()),
            rows=[dict(ts=ts,text=V['O']['received']()['text']+' Preserved original context' if ts==V['TS'] else 'Original root') for ts in c['visibleTs']])
        rows.append(dict(stage=name,proof=p,model=dict(comments=m,fixture=f,delete=delete,editFixture=copy.deepcopy(first['model']['fixture'])),
            geometry=g,journal=copy.deepcopy(first['journal']),deleteInventory=inv,capture=str(DIRECTORY/f'{fixture}-{name}.png')))
    return fixture,dict(ok=True,case=case,fixture=fixture,readiness=base['readiness'],stages=rows,final=copy.deepcopy(rows[-1]['model']),initialRecords=base['initialRecords'],noLiveNetwork=True)


class OwnDeleteTests(unittest.TestCase):
    def check(self,fixture,receipt):
        return V['validator'](receipt['case'])(fixture,receipt,DIRECTORY,png=False)

    def reject(self,change,i=3,case='own-delete'):
        fixture,r=sample(case)
        change(r['stages'][i])
        with self.assertRaises((ValueError,AssertionError,RuntimeError)):
            self.check(fixture,r)

    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):self.check(*sample(case,narrow))

    def test_shared_runner_routes_exact_cases(self):
        runner=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
        for case in V['STAGES']:
            names,validate=runner['case_contract'](case)
            self.assertEqual(names,V['STAGES'][case]);f,r=sample(case);validate(f,r,DIRECTORY,png=False)

    def test_no_delete_before_explicit_confirm(self):
        self.reject(lambda r:r['model']['fixture']['deletes'].append(V['wire']()),1)
        self.reject(lambda r:r['deleteInventory'].update(attempt={'phase':'Pending'}),2)

    def test_wire_is_exact_one_original_ordinary_reply(self):
        for field,value in [('channel','COTHER'),('ts',V['READ']['ROOT_TS']),('as_user','true')]:
            self.reject(lambda r,f=field,v=value:r['model']['fixture']['deletes'][0]['args'].update({f:v}))
        self.reject(lambda r:r['model']['fixture']['deletes'].append(V['wire']()))
        self.reject(lambda r:r['model']['fixture']['deletes'][0].update(path='/api/chat.update'))

    def test_permissions_are_scoped_positive_and_fresh(self):
        self.reject(lambda r:r['model']['fixture']['policy'][1]['request']['args'].update(user_id='UOTHER'))
        self.reject(lambda r:r['model']['fixture']['policy'][1]['response'].update(allowed=[]))
        self.reject(lambda r:r['model']['fixture']['policy'].pop())

    def test_reply_content_scope_and_original_mount_survive(self):
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=False))
        self.reject(lambda r:r['geometry']['reply']['blocks'][0]['elements'][0]['elements'][1]['opaque'].update(retain=[]))
        self.reject(lambda r:r['journal'].update(revision=999))
        self.reject(lambda r:r['journal']['scope'].update(user='UOTHER'))

    def test_hidden_reply_not_claimed_as_visible_pixels(self):
        self.reject(lambda r:r['geometry']['reply'].update(hidden=False),1)
        self.reject(lambda r:r['geometry']['reply'].update(inert=False))
        self.reject(lambda r:r['geometry'].update(text=B['B']['textproof']()),1)
        self.reject(lambda r:r['geometry']['panel']['preview'].update(visible=False),1)

    def test_held_disabled_and_exact_preview(self):
        self.reject(lambda r:r['geometry']['controls'][-1].update(disabled=False))
        self.reject(lambda r:r['geometry']['panel']['preview'].update(bold=[]))
        self.reject(lambda r:r['geometry']['panel']['preview'].update(actions=1))
        self.reject(lambda r:r['model']['delete']['active'].update(sourceCurrent=False))

    def test_original_receipt_cannot_be_substituted(self):
        self.reject(lambda r:r['model']['fixture']['responses'][0]['raw'].update(channel='COTHER'),4)
        self.reject(lambda r:r['deleteInventory']['attempt']['raw'].update(ts=V['READ']['ROOT_TS']),4)
        self.reject(lambda r:r['deleteInventory']['attempt']['record']['message'].update(text='Changed'),4)

    def test_unknown_has_no_ack_no_replay_and_no_early_row_removal(self):
        self.reject(lambda r:r['deleteInventory']['attempt'].update(raw=V['ACK']),2,'own-delete-failure')
        self.reject(lambda r:r['model']['delete']['active'].update(canConfirm=True),2,'own-delete-failure')
        self.reject(lambda r:r['model']['comments']['comments'].update(visibleTs=[V['READ']['ROOT_TS']]),2,'own-delete-failure')
        self.reject(lambda r:r['model']['fixture']['deletes'].append(V['wire']()),3,'own-delete-failure')

    def test_observed_requires_positive_original_readback_not_partial_absence(self):
        self.reject(lambda r:r['deleteInventory']['attempt']['readback']['raw'].pop('deleted'),4,'own-delete-failure')
        self.reject(lambda r:r['deleteInventory']['attempt']['readback']['raw']['messages'].append(V['O']['received']()),4,'own-delete-failure')
        self.reject(lambda r:r['deleteInventory']['attempt']['readback']['target'].update(channel='COTHER'),4,'own-delete-failure')
        self.reject(lambda r:r['deleteInventory']['attempt'].update(phase='Confirmed',raw=V['ACK']),4,'own-delete-failure')

    def test_only_typed_row_changes_before_canonical_read(self):
        self.reject(lambda r:r['model']['comments']['comments']['page']['messages'].pop(),4)
        self.reject(lambda r:r['model']['comments']['comments']['pageMessages'].append(V['O']['received']()),4)
        self.reject(lambda r:r['geometry']['rows'].append(dict(ts=V['TS'],text='Resurrected')),4)
        self.reject(lambda r:r['model']['comments']['comments']['pageMessages'][0].update(text='Foreign root'),4)

    def test_no_unrelated_reply_edit_reaction_or_list_write(self):
        self.reject(lambda r:r['model']['comments']['composeLedger']['posts'].append({}))
        self.reject(lambda r:r['model']['editFixture']['updates'].append({}))
        self.reject(lambda r:r['model']['comments']['reactionLedger']['issued'].append({}))
        self.reject(lambda r:r['model']['comments'].update(writes=1))

    def test_reopen_requires_fresh_runtime_binding_same_disk_attempt(self):
        self.reject(lambda r:r['proof'].update(runtimeForgotten=False),6)
        self.reject(lambda r:r['deleteInventory']['attempt'].update(id='00000000-0000-4000-8000-000000000099'),6)
        self.reject(lambda r:r['geometry']['reply'].update(sameNode=True),6)

    def test_png_dimensions_remain_physical_not_css_viewport(self):
        f,r=sample();saved=V['H']['png_signature'];calls=[]
        V['H']['png_signature']=lambda path,dimensions:calls.append((path,dimensions))
        try:V['validator']('own-delete')(f,r,DIRECTORY,dimensions=(1942,1256))
        finally:V['H']['png_signature']=saved
        self.assertEqual(len(calls),7)
        self.assertTrue(all(d==(1942,1256) for _,d in calls))


if __name__=='__main__':unittest.main()
