"""Independent native-shape mutations and bounded run-comparison vectors."""
import copy
from pathlib import Path
import runpy
import unittest

V=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_runs.py')))
B=runpy.run_path(str(Path(__file__).with_name('test_slack_list_own_edit_noop.py')))
O,T=V['O'],V['T'];D=Path('/private/own-edit-runs')


def split(italic=False):
    value=copy.deepcopy(V['ORIGINAL']);style={'bold':True}
    value[0]['elements'][0]['elements']=[dict(type='text',text='Original ',style=copy.deepcopy(style)),
        dict(type='text',text='own',style=dict(style,**({'italic':True} if italic else {}))),
        dict(type='text',text=' comment 中🙂.',style=copy.deepcopy(style))]
    return value


def sample(narrow=False):
    fixture,base=B['sample'](narrow=narrow);_,storage,_=B['B']['sample'](narrow)
    first=base['stages'][0];scope=first['journal']['scope'];seed=T['RST']['seed_value'](first['model'])
    on,off=split(True),split(False);rows=[];p={k:copy.deepcopy(first['proof'][k]) for k in ('source','replySeed','replyBlocks','storageControls')}
    opens=[];actions=[];readset=first['model']['comments']['ledger']['requests']
    def inv(i,blocks,active=False,attempt=None):
        return dict(scope=copy.deepcopy(scope),ts=O['TS'],revision=i,capture=f'00000000-0000-4000-8000-{i:012d}' if i else '',
            seed=copy.deepcopy(seed) if active else None,blocks=copy.deepcopy(blocks),attempt=copy.deepcopy(attempt),retry_at=0)
    def owner(mount,blocks,action=None):
        value=dict(scope=mount['scope'],generation=mount['generation'],sameNode=True,native=dict(blocks=copy.deepcopy(blocks)),selected='own')
        if action is not None:value.update(ok=True,action=action)
        return value
    def select(mount):
        return dict(owner(mount,V['ORIGINAL'],'select'),anchorOffset=9,focusOffset=12,textNode=V['ORIGINAL'][0]['elements'][0]['elements'][0]['text'])
    def click(mount,blocks):
        return dict(ok=True,action='italic',before=owner(mount,blocks),control=dict(label='Italic',disabled=False,sameNode=True,visible=True,hit=True,rect=B['B']['B']['rect']()))
    ack=dict(ok=True,channel=O['CHANNEL'],ts=O['TS'],message=O['update_row'](on))
    pending=dict(id='00000000-0000-4000-8000-000000000080',record=dict(version=1,seed=copy.deepcopy(seed),blocks=copy.deepcopy(on)+O['received']()['blocks'][1:],text=V['G']['display'](on)),
        capture='00000000-0000-4000-8000-000000000006',phase='Pending',raw=None,readback=None,retry_at=0)
    for i,name in enumerate(V['STAGES']):
        source=0 if i==0 else 1 if i<4 else 2 if i==4 else 3 if i==5 else 2
        row=copy.deepcopy(base['stages'][source]);row.update(stage=name,capture=str(D/f'{fixture}-{name}.png'))
        m=row['model'];c,f,e=(m[k] for k in ('comments','fixture','edit'));n=V['READS'][i]
        c['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=j+1) for j,q in enumerate(readset*(n//4))],completed=n,replies=n//4,cancelled=0,held=False)
        f['policy']=[dict(request=q,response=r) for _ in range(V['POLICIES'][i]) for q,r in O['policy_cycle']()]
        blocks=V['ORIGINAL'] if i==1 else on if i in (2,5,6) else off if i==3 else None
        if i in (1,5):
            op=copy.deepcopy(base['stages'][1 if i==1 else 3]['proof']['opens'][-1]);opens.append(op)
            actions.append(copy.deepcopy(base['stages'][1]['proof']['actions'][0]));p['originalBlocks']=copy.deepcopy(V['ORIGINAL'])
        mount=opens[-1]['mount'] if opens else None
        if blocks is not None:
            if i==6:
                m['edit']=copy.deepcopy(storage['stages'][9]['model']['edit']);m['capture']=copy.deepcopy(storage['stages'][9]['model']['capture']);e=m['edit']
            a=e['active'];a.update(scope=mount['scope'],generation=int(mount['generation']),blocks=copy.deepcopy(blocks),revision=i,busy=i==6,
                notice='Saving the edited comment…' if i==6 else None,attempt=123 if i==6 else None)
            m['capture'].update(scope=mount['scope'],generation=int(mount['generation']),mount=mount['mount'],canSave=i!=6,canCancel=i!=6,busy=i==6)
        else:e['active']=None;m['capture']=None
        if i>=6:
            outgoing=O['wire'](on);f.update(prepared=[outgoing],updates=[copy.deepcopy(outgoing)],responses=[dict(disposition='accepted',raw=copy.deepcopy(ack))],futureRows=[copy.deepcopy(ack['message'])],
                held=i==6,releases=[] if i==6 else [O['RELEASE']],published=i==8,publishedRow=copy.deepcopy(ack['message']) if i==8 else None)
            e['attempts']=[dict(id=123,ts=O['TS'],blocks=copy.deepcopy(on)+O['received']()['blocks'][1:],phase='sent' if i==6 else 'confirmed',receipt=None if i==6 else copy.deepcopy(ack),retryAt=0)]
            if i>=7:
                typed=O['one_message'](c['comments']['pageMessages'])
                for key in ('text','blocks','edited'):typed[key]=copy.deepcopy(ack['message'][key])
            if i==8:c['comments']['page']['messages']=[copy.deepcopy(ack['message']) if v.get('ts')==O['TS'] else v for v in c['comments']['page']['messages']]
        row['editInventory']=inv(i,blocks or [],blocks is not None,pending if i==6 else None)
        g=row['geometry'];g.update(editing=blocks is not None,edit=dict(blocks=copy.deepcopy(blocks),editable=i!=6,sameNode=True,rect=B['B']['B']['rect']()) if blocks is not None else None,
            controls=[dict(id=id,text=id,disabled=i==6,visible=True,hit=True,rect=B['B']['B']['rect']()) for id in (['slack-list-comment-edit-cancel','slack-list-comment-edit-save'] if blocks is not None else ['slack-list-comments-back','slack-list-comment-send'])],notice=None)
        g['reply'].update(visible=blocks is None,hidden=blocks is not None,inert=blocks is not None)
        if i==6:
            sg=storage['stages'][9]['geometry'];g.update(text=copy.deepcopy(sg['text']),inertTextPaint=copy.deepcopy(sg['inertTextPaint']),notice=copy.deepcopy(sg['notice']))
        if i>=6:row.pop('rowNative',None)
        if i==2:p['formatting']=dict(selection=select(mount),onClick=click(mount,V['ORIGINAL']),on=owner(mount,on,'snapshot'))
        if i==3:p['formatting'].update(offClick=click(mount,on),off=owner(mount,off,'snapshot'))
        if i in (4,6,8):
            id='slack-list-comments-refresh' if i==8 else 'slack-list-comment-edit-save'
            actions.append(dict(id=id,native=dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=B['B']['B']['rect']()))))
        if i==4:p.update(saveBefore=copy.deepcopy(rows[3]['model']),saveAfter=copy.deepcopy(m))
        if i==5:p.update(changedSelection=select(mount),changedClick=click(mount,V['ORIGINAL']),changed=owner(mount,on,'snapshot'))
        if i==6:p['doubleSave']=dict(ok=True,disabled=True)
        if i==7:p['release']=O['RELEASE']
        p.update(actions=copy.deepcopy(actions),opens=copy.deepcopy(opens));row['proof']=copy.deepcopy(p);rows.append(row)
    result=dict(ok=True,case=V['CASE'],fixture=fixture,readiness=base['readiness'],initialRecords=base['initialRecords'],noLiveNetwork=True,stages=rows,proof=copy.deepcopy(p),final=copy.deepcopy(rows[-1]['model']))
    final_attempt=copy.deepcopy(pending);final_attempt.update(phase='Confirmed',raw=ack)
    closed=dict(account=first['model']['comments']['account'],inventories=[first['journal']],editInventories=[rows[-1]['editInventory']],editHistory=[final_attempt],contentSha256='a'*64)
    return fixture,result,closed


class RunsTests(unittest.TestCase):
    def reject(self,mutation):
        f,r,_=sample();mutation(r)
        with self.assertRaises((RuntimeError,KeyError,TypeError,IndexError,ValueError)):V['validate'](f,r,D,png=False)

    def test_both_widths_and_closed_history(self):
        for narrow in (False,True):
            f,r,c=sample(narrow);V['validate'](f,r,D,png=False);V['validate_closed'](c,r)

    def test_real_fragmentation_required(self):
        self.reject(lambda r:r['stages'][3]['proof']['formatting']['off']['native'].update(blocks=copy.deepcopy(V['ORIGINAL'])))

    def test_noop_cannot_change_style_text_opaque_or_parent(self):
        for field,value in (('text','wrong'),('style',{'bold':1}),('opaque',True)):
            with self.subTest(field=field):self.reject(lambda r:r['stages'][3]['proof']['formatting']['off']['native']['blocks'][0]['elements'][0]['elements'][0].update({field:value}))
        self.reject(lambda r:r['stages'][3]['proof']['formatting']['off']['native']['blocks'][0].update(block_id='foreign'))

    def test_original_selection_and_toolbar_exposure(self):
        self.reject(lambda r:r['stages'][2]['proof']['formatting']['selection'].update(anchorOffset=8))
        self.reject(lambda r:r['stages'][2]['proof']['formatting']['onClick']['control'].update(hit=False))
        self.reject(lambda r:r['stages'][3]['proof']['formatting']['offClick']['before'].update(sameNode=False))

    def test_old_mount_cannot_authorize_changed_edit(self):
        self.reject(lambda r:r['stages'][5]['proof']['changed'].update(scope=r['stages'][1]['proof']['opens'][0]['mount']['scope']))

    def test_equivalent_save_cannot_send_or_preflight(self):
        self.reject(lambda r:r['stages'][4]['model']['fixture']['updates'].append({}))
        self.reject(lambda r:r['stages'][4]['proof']['saveAfter']['fixture']['policy'].append({}))
        self.reject(lambda r:r['stages'][4]['model']['edit']['attempts'].append({}))

    def test_retained_format_is_positive_change(self):
        self.reject(lambda r:r['stages'][5]['proof']['changed']['native'].update(blocks=split(False)))
        self.reject(lambda r:r['stages'][5]['proof']['changed']['native']['blocks'][0]['elements'][0]['elements'][1].update(text='other'))

    def test_original_update_wire_ack_suffix_and_readback(self):
        self.reject(lambda r:r['stages'][6]['model']['fixture']['updates'][0]['args'].update(channel='FOREIGN'))
        self.reject(lambda r:r['stages'][7]['model']['fixture']['responses'][0]['raw']['message']['blocks'].pop())
        self.reject(lambda r:r['stages'][8]['model']['comments']['comments']['page']['messages'].pop())

    def test_pending_double_save_and_attempt_identity(self):
        self.reject(lambda r:r['stages'][6]['proof']['doubleSave'].update(disabled=False))
        self.reject(lambda r:r['stages'][6]['model']['edit']['attempts'].append({}))
        self.reject(lambda r:r['stages'][6]['model']['capture'].update(canCancel=True))

    def test_independent_reply_native_and_durable(self):
        self.reject(lambda r:r['stages'][4]['geometry']['reply'].update(sameNode=False))
        self.reject(lambda r:r['stages'][6]['journal'].update(revision=999))
        self.reject(lambda r:r['stages'][6]['editInventory'].update(attempt=None))

    def test_closed_history_contains_only_changed_ack(self):
        _,r,c=sample()
        for key,value in (('editHistory',[]),('editHistory',c['editHistory']*2),('inventories',[])):
            with self.subTest(key=key),self.assertRaises(RuntimeError):V['validate_closed'](dict(c,**{key:value}),r)

    def test_unicode_coalescence_keeps_inputs(self):
        a=split(False);before=copy.deepcopy(a);V['segmentation'](a);self.assertEqual(a,before)
        self.assertEqual(V['coalesced'](a),V['ORIGINAL'])

    def test_exact_style_representations(self):
        for style in ({},{'bold':False},{'bold':1},{'bold':None},{'unknown':True},None):
            v=split(False);v[0]['elements'][0]['elements'][1]['style']=style
            with self.subTest(style=style),self.assertRaises(RuntimeError):V['segmentation'](v)

    def test_semantic_opaque_and_newline_barriers(self):
        for node in ({'type':'user','user_id':'U1'},{'type':'text','text':'','style':{'bold':True}},
                     {'type':'text','text':'\n','style':{'bold':True}},{'type':'text','text':'own','opaque':True,'style':{'bold':True}}):
            v=split(False);v[0]['elements'][0]['elements'].insert(1,node)
            with self.subTest(node=node),self.assertRaises(RuntimeError):V['segmentation'](v)

    def test_quote_and_opaque_shapes_are_not_traversed(self):
        v=split(False);section=v[0]['elements'][0]
        quote=[dict(type='rich_text_quote',elements=[section])]
        opaque=[dict(type='rich_text',elements=[],opaque=section)]
        self.assertEqual(V['coalesced'](quote),quote);self.assertEqual(V['coalesced'](opaque),opaque)

    def test_sections_lists_and_blocks_keep_boundaries(self):
        v=split(False);v[0]['elements'][0]['elements'].pop()
        v[0]['elements'].insert(1,dict(type='rich_text_section',elements=[dict(type='text',text=' comment 中🙂.',style={'bold':True})]))
        with self.assertRaises(RuntimeError):V['segmentation'](v)
        a=[dict(type='rich_text',elements=[dict(type='rich_text_list',style='bullet',elements=[split(False)[0]['elements'][0]])])]
        b=copy.deepcopy(a);b[0]['elements'][0]['elements'][0]=copy.deepcopy(V['ORIGINAL'][0]['elements'][0])
        self.assertEqual(V['coalesced'](a),b)

    def test_isolated_runner_keeps_baseline_contract_and_zero_commands(self):
        w=runpy.run_path(str(Path(__file__).with_name('check-list-own-edit-runs-native.py')));h=w['harness']()
        self.assertEqual(h['case_contract'](V['CASE'])[0],V['STAGES']);self.assertEqual(h['case_contract']('own-edit')[0],O['STAGES']['own-edit'])
        cls=w['storage_controller'](V['CASE'])['StorageControl'];c=cls(Path('/not-entered'),{},case=V['CASE'])
        self.assertEqual(c.command_count,0);self.assertEqual(c.case,'own-edit-storage')
        with self.assertRaises(RuntimeError):cls(Path('/not-entered'),{},case='own-edit')


if __name__=='__main__':unittest.main()
