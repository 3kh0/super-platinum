"""Bounded mutation corpus; modeled receipts are not native acceptance."""
import copy
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
import canvas_comment_emoji as o
from native_check import expected_viewport


def exposed():
    return dict(rect=dict(x=20,y=20,width=100,height=24),hitPoint=dict(x=50,y=30),exposed=True)


def sample(fixture=o.PREFIX):
    viewport=expected_viewport(fixture)
    blocks=[dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[dict(type='text',text=o.DRAFT)])]), dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[])])]
    runtime=dict(id='entry-1',account=o.ACCOUNT,workspace=o.TEAM,user=o.USER,file='Ffixture',root=o.ROOT,text=o.DRAFT+'\n',blocks=blocks,capture='capture-1',serial=1,loading=False,saving=False,sending=False,error=None,nativePresent=True)
    draft=dict(node=30,mount='native-mount',scope='native-scope',generation=1,blocks=blocks,composing=False,anchor=31,focus=31,start=2,end=7,connected=True,**exposed())
    reply=dict(runtime=runtime,journal=dict(drafts=[dict(id='draft-1',revision=1,capture='capture-1',text=o.DRAFT+'\n',blocks=blocks)],attempt=None,history=[]),ledger=dict(requests=[],prepared=[]))
    platform='windows' if sys.platform=='win32' else 'darwin' if sys.platform=='darwin' else 'linux'
    assignments=[dict(serial=j+1,socket=7+j,method='POST',path='/api/experiments.getByUser',args=dict(ssb_platform=platform,ssb_release_channel='prod'),tokenPresent=True,factoryMatched=True,response=o.RAW) for j in range(2)]
    ws=[dict(current=True,version='v17',response=o.RAW,owner=dict(account=o.ACCOUNT,epoch=2,team=o.TEAM,user=o.USER,socket=7+j,slotSerial=10+j)) for j in range(2)]
    reads=[]
    for j in range(2):
        fields=(dict(channel='Cfixture',ts=o.ROOT,limit='28',inclusive='true',cached_latest_updates='{}') if j else dict(channel='Cfixture',limit='20',document_stream='latest_reply',ignore_replies='true',inclusive='true',include_pin_count='true',no_user_profile='true',include_stories='true',include_free_team_extra_messages='true',include_date_joined='false',cached_latest_updates='{}'))
        reads.append(dict(serial=j+1,method='POST',url='https://example.test/api/'+('conversations.replies' if j else 'conversations.history'),fields=list(map(list,fields.items())),tokenPresent=True,response=dict(ok=True,has_more=False,messages=o.messages(bool(j)))))
    proof=dict(stream=dict(ok=True,panel=1,root=2,preview=3))
    stages=[]
    for i,name in enumerate(o.STAGES):
        count=(0,1,1,1,1,2)[i]
        witnesses=copy.deepcopy(ws[:count])
        if i>=4:witnesses[0]['current']=False
        selected=copy.deepcopy(witnesses[-1]) if i in (1,2,3,5) else None
        retained_rows=[]
        for message in [v for v in o.messages(i>=3) if i>=3 or v['ts']!=o.REPLY]:
            message=copy.deepcopy(message)
            if 'files' in message: message['files']=[o.typed_file(f) for f in message['files']]
            retained_rows.append(dict(message=message,preview=copy.deepcopy(o.messages()[1]) if i<3 and message['ts']==o.ROOT else None,body='original typed body',previewBody=None,file=dict(id='FCANEMOJI',name='Original canonical file',url='https://example.test/canonical/FCANEMOJI',ts=o.FILE) if message['ts']==o.FILE else None))
        model=dict(contextCurrent=True,account=o.ACCOUNT,epoch=2,team=o.TEAM,user=o.USER,socket=7 if i<4 else 8,visit=3,boot=o.boot(),requests=copy.deepcopy(assignments[:count]),reads=copy.deepcopy(reads[:1 if i<3 else 2]),witnesses=witnesses,selected=selected,customEmoji={},customRevision=0,mediaEpoch=1 if i<2 else 2,registered=dict(known=int(i>0),hidden=0,file=0,unknown=0,invalid=0),retained=dict(serial=1 if i<3 else 2,query='Some(Stream(Recent))' if i<3 else 'Some(Replies("'+o.ROOT+'"))',ownerCurrent=True,owner=dict(file='Ffixture',visit=3,team=o.TEAM),loading=False,error=None,rows=retained_rows,root=None if i<3 else copy.deepcopy(o.messages()[0]),selected=copy.deepcopy(selected)))
        isselected=i in (1,2,3,5);ready=i in (2,3,5)
        glyphs=[dict(text='A',title=':unknown_original:')]
        if not isselected:glyphs.insert(0,dict(text='\U0001f44d\U0001f3fd',title=':conflicting_original:'))
        elif not ready:glyphs.insert(0,dict(text=':+1::skin-tone-4:',title=':+1::skin-tone-4:'))
        def body(label,node):
            return dict(node=node,text=label+' '+('' if ready else ':+1::skin-tone-4:' if isselected else '\U0001f44d\U0001f3fd')+' unknown A invalid ',glyphs=copy.deepcopy(glyphs),images=[dict(src='super-platinum-media://emoji/asset?v=2',alt=':+1::skin-tone-4:',title=':+1::skin-tone-4:',complete=True,width=16,height=16,**exposed())] if ready else [],links=[],**exposed())
        rows=[dict(ts=o.ROOT,node=2 if i<3 else 20,body=body('Root',4 if i<3 else 21),preview=body('Last reply',5) if i<3 else None)]
        if i<3:
            rows.extend([dict(ts=o.HIDDEN,node=6,body=dict(node=7,text='This content was deleted.',images=[],glyphs=[],links=[],**exposed()),preview=None),dict(ts=o.FILE,node=8,body=dict(node=9,text='Original canonical file',images=[],glyphs=[],links=[dict(id='FCANEMOJI',root=o.FILE,name='Original canonical file',url='https://example.test/canonical/FCANEMOJI',text='Original canonical file')],**exposed()),preview=None)])
        else:rows.append(dict(ts=o.REPLY,node=22,body=body('Last reply',23),preview=None))
        layout=dict(ok=True,viewport=viewport,panel=1,stream=i<3,sameOriginal=True,rows=rows,draft=copy.deepcopy(draft) if i>=3 else None,scroll=dict(top=0,left=0))
        if i==1:proof['assignment1']=copy.deepcopy(selected)
        if i==2:proof['media']=dict(url=o.URL,id='super-platinum-media://emoji/asset',uriBefore='super-platinum-media://emoji/asset?v=1',epochBefore=1,bytes=len((Path(__file__).resolve().parent.parent/'assets/icons/png/icon_16.png').read_bytes()),mime='image/png',readyBefore=False,readyAfter=True)
        if i==3:
            proof['input']=dict(ok=True,action='synthetic-native-insertText',isTrusted=False,text=o.DRAFT,node=30,before=exposed())
            proof['draft']=dict(ok=True,**{k:draft[k] for k in ('node','anchor','focus','start','end','scope','generation','mount','blocks')})
        if i==4:proof['retirement']=dict(kind='synthetic-socket-connected',before=copy.deepcopy(ws[0]),after=dict(copy.deepcopy(ws[0]),current=False),socket=8,accessorAvailable=False,networkDispatched=False,workResumed=False)
        if i==5:proof['assignment2']=copy.deepcopy(selected)
        stages.append(dict(ok=True,stage=name,ordinal=i,noSession=False,noTransport=False,isolation=copy.deepcopy(o.ISOLATION),model=model,layout=layout,reply=copy.deepcopy(reply) if i>=3 else None,proof=copy.deepcopy(proof)))
    if fixture.endswith('-narrow'):
        expanded=[]
        for ordinal,(logical,view,index) in enumerate(o.stage_plan(fixture)):
            row=copy.deepcopy(stages[index]);layout=row['layout']
            row.update(stage=logical+'-'+view,logicalStage=logical,view=view,ordinal=ordinal)
            keyed={r['ts']:r for r in layout['rows']}
            targets={'root':keyed[o.ROOT]['body']}
            if index<3:targets['preview']=keyed[o.ROOT]['preview']
            else:targets.update(reply=keyed[o.REPLY]['body'],editor=layout['draft'])
            for key,body in targets.items():
                if key!=view:
                    body.update(exposed=False,hitPoint=None)
                    for img in body.get('images',[]):img.update(exposed=False,hitPoint=None)
            target=targets[view]
            selection=dict(active=30,anchor=31,focus=31,start=2,end=7) if index>=3 else dict(active=1,anchor=None,focus=None,start=0,end=0)
            scroll=dict(top={'root':0,'preview':50,'reply':60,'editor':100}[view],left=0)
            layout.update(view=view,target=target['node'],scroll=scroll)
            row['exposure']=dict(ok=True,action='scroll-original-target',view=view,target=target['node'],before=selection,after=copy.deepcopy(selection),scrollBefore=dict(top=0,left=0),scrollAfter=copy.deepcopy(scroll),**exposed())
            expanded.append(row)
        stages=expanded
    return dict(ok=True,noSession=False,noTransport=False,isolation=copy.deepcopy(o.ISOLATION),stages=stages,layout=copy.deepcopy(stages[-1]['layout']),model=copy.deepcopy(stages[-1]['model']),reply=copy.deepcopy(stages[-1]['reply']),syntheticInput=True,requestCount=2,previewRetirementNative=False)


class OracleTests(unittest.TestCase):
    def test_normal(self):o.validate(sample(),o.PREFIX)
    def test_narrow(self):o.validate(sample(o.FIXTURES[1]),o.FIXTURES[1])
    def test_narrow_requires_all_original_exposures(self):
        value=sample(o.FIXTURES[1]);value['stages'].pop(1)
        with self.assertRaises(RuntimeError):o.validate(value,o.FIXTURES[1])
    def test_narrow_cannot_use_normal_simultaneous_receipts(self):
        with self.assertRaises(RuntimeError):o.validate(sample(),o.FIXTURES[1])
    def test_normal_cannot_use_sequential_receipts(self):
        with self.assertRaises(RuntimeError):o.validate(sample(o.FIXTURES[1]),o.PREFIX)
    def test_width_specs_cannot_be_swapped(self):
        spec=importlib.util.spec_from_file_location('canvas_emoji_wrapper',Path(__file__).with_name('check-canvas-comment-emoji-native.py'));w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)
        self.assertTrue(w.paired(w.SPEC,o.PREFIX));self.assertTrue(w.paired(w.NARROW_SPEC,o.FIXTURES[1]))
        self.assertFalse(w.paired(w.SPEC,o.FIXTURES[1]));self.assertFalse(w.paired(w.NARROW_SPEC,o.PREFIX))
        with tempfile.TemporaryDirectory() as d:
            owner=w.prepare(w.NARROW_SPEC,o.FIXTURES[1],Path(d),{o.FLAG:'1'})
            self.assertEqual(owner['fixture'],o.FIXTURES[1])
    def test_original_empty_section_cannot_be_removed_consistently(self):
        value=sample()
        def remove(node):
            if isinstance(node,dict):
                if 'blocks' in node and node['blocks']==o.captured_blocks():node['blocks']=node['blocks'][:1]
                for child in node.values():remove(child)
            elif isinstance(node,list):
                for child in node:remove(child)
        remove(value)
        with self.assertRaises(RuntimeError):o.validate(value,o.PREFIX)
    def test_original_empty_section_cannot_gain_whitespace(self):
        value=sample()
        def change(node):
            if isinstance(node,dict):
                if 'blocks' in node and node['blocks']==o.captured_blocks():
                    node['blocks']=copy.deepcopy(node['blocks']);node['blocks'][1]['elements'][0]['elements']=[dict(type='text',text=' ')]
                for child in node.values():change(child)
            elif isinstance(node,list):
                for child in node:change(child)
        change(value)
        with self.assertRaises(RuntimeError):o.validate(value,o.PREFIX)
    def test_original_fallback_newline_cannot_be_trimmed(self):
        value=sample()
        for row in value['stages'][3:]:
            row['reply']['runtime']['text']=o.DRAFT
            row['reply']['journal']['drafts'][0]['text']=o.DRAFT
        with self.assertRaises(RuntimeError):o.validate(value,o.PREFIX)
    def test_readiness(self):
        spec=importlib.util.spec_from_file_location('canvas_emoji_wrapper',Path(__file__).with_name('check-canvas-comment-emoji-native.py'));w=importlib.util.module_from_spec(spec);spec.loader.exec_module(w)
        with tempfile.TemporaryDirectory() as d:
            checks={o.FLAG:'1'};owner=w.prepare(w.SPEC,o.PREFIX,Path(d),checks)
            self.assertEqual(owner['fixture'],o.PREFIX);self.assertEqual(len(owner['nonce']),32)
            with self.assertRaises(RuntimeError):w.prepare(w.SPEC,o.PREFIX,Path(d),checks)
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(RuntimeError):w.prepare(w.SPEC,'canvas-comments',Path(d),{o.FLAG:'1'})
    def test_exact_gate(self):
        text=(Path(__file__).resolve().parent.parent/'src/desktop/canvas/comments/emoji_fixture.rs').read_text()
        self.assertIn('feature = "protected-native-fixture"',text);self.assertIn(o.FLAG,text)
        self.assertIn('t.http().is_err()',text);self.assertIn('Arc::ptr_eq',text)
        self.assertIn('shell.core.cache.is_some()',text)


def mutate_path(value,path,changed):
    for k in path[:-1]:value=value[k]
    value[path[-1]]=changed

mutations={
 'fabricated_sessionless':(['noSession'],True),
 'network_capability':(['stages',0,'isolation','noHttp'],False),
 'missing_stage':(['stages',5,'stage'],'socket-retired'),
 'hidden_media':(['stages',1,'model','registered','hidden'],1),
 'override_media':(['stages',2,'model','registered','file'],1),
 'default_map':(['stages',0,'model','selected'],{}),
 'stale_map':(['stages',4,'model','witnesses',0,'current'],True),
 'foreign_map_owner':(['stages',1,'model','witnesses',0,'owner','account'],'foreign'),
 'wrong_factory':(['stages',1,'model','requests',0,'path'],'/api/users.info'),
 'extra_read':(['stages',2,'model','reads',0,'fields'],[['channel','Cforeign']]),
 'raw_url':(['stages',0,'model','reads',0,'response','messages',0,'blocks',0,'elements',0,'elements',1,'display_url'],'https://other.invalid/'),
 'raw_opaque_type':(['stages',0,'model','reads',0,'response','messages',0,'blocks',0,'opaque','keep',0],0),
 'preview_raw':(['stages',1,'model','retained','rows',0,'preview','text'],'replaced'),
 'retained_owner':(['stages',3,'model','retained','ownerCurrent'],False),
 'extra_catalog':(['stages',2,'model','customEmoji'],{'new':'https://fixture.invalid/x'}),
 'wrong_unicode':(['stages',0,'layout','rows',0,'body','glyphs',0,'text'],'\U0001f44d'),
 'wrong_image_label':(['stages',2,'layout','rows',0,'body','images',0,'alt'],':wrong_alias:'),
 'external_image':(['stages',2,'layout','rows',0,'body','images',0,'src'],o.URL),
 'broken_image':(['stages',2,'layout','rows',0,'body','images',0,'width'],0),
 'preview_node':(['stages',2,'layout','rows',0,'preview','node'],999),
 'hidden_dom':(['stages',0,'layout','rows',1,'body','text'],'Hidden'),
 'file_override_href':(['stages',1,'layout','rows',2,'body','links',0,'url'],'https://evil.invalid'),
 'reply_caret':(['stages',4,'layout','draft','end'],8),
 'reply_holder':(['stages',4,'layout','draft','mount'],'successor'),
 'reply_disk_capture':(['stages',4,'reply','journal','drafts',0,'capture'],'other'),
 'reply_send':(['stages',3,'reply','ledger','prepared'],[{'method':'POST'}]),
 'reply_history':(['stages',5,'reply','journal','history'],[{}]),
 'fake_input':(['stages',3,'proof','input','isTrusted'],True),
 'unexposed_input':(['stages',3,'proof','input','before','exposed'],False),
 'retirement_dispatch':(['stages',4,'proof','retirement','networkDispatched'],True),
 'invented_preview_claim':(['previewRetirementNative'],True),
}
for name,(path,changed) in mutations.items():
    def test(self,path=path,changed=changed):
        value=sample();mutate_path(value,path,changed)
        with self.assertRaises(RuntimeError):o.validate(value,o.PREFIX)
    setattr(OracleTests,'test_reject_'+name,test)

narrow_mutations={
 'target_offscreen':(['stages',1,'layout','rows',0,'preview','exposed'],False),
 'image_offscreen':(['stages',5,'layout','rows',0,'preview','images',0,'exposed'],False),
 'wrong_target_node':(['stages',1,'exposure','target'],999),
 'wrong_target_view':(['stages',1,'layout','view'],'root'),
 'focus_changed':(['stages',6,'exposure','after','active'],99),
 'caret_changed':(['stages',6,'exposure','after','end'],8),
 'wrong_original_caret':(['stages',7,'exposure','before','start'],0),
 'editor_remount':(['stages',8,'layout','draft','mount'],'replacement'),
 'disk_capture_changed':(['stages',7,'reply','journal','drafts',0,'capture'],'replacement'),
 'reordered_capture':(['stages',1,'stage'],'stream-unobserved-root'),
 'scroll_missing':(['stages',1,'exposure','scrollAfter'],{}),
}
for name,(path,changed) in narrow_mutations.items():
    def test(self,path=path,changed=changed):
        value=sample(o.FIXTURES[1]);mutate_path(value,path,changed)
        with self.assertRaises(RuntimeError):o.validate(value,o.FIXTURES[1])
    setattr(OracleTests,'test_narrow_reject_'+name,test)

if __name__=='__main__':unittest.main()
