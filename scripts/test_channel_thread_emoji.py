import copy
import importlib.util
import sys
import unittest
from pathlib import Path
import channel_thread_emoji as O

def sample(narrow=False):
    fixture=O.PREFIX+('-narrow' if narrow else '')
    rect=dict(x=10,y=50,width=180,height=25)
    def exposure():return dict(rect=rect,exposed=True,hitPoint=dict(x=40,y=60))
    channel=O.CHANNEL_DRAFT+('' if narrow else ' edited')
    def composer(text,start,end,scope,mount):return dict(value=text,start=start,end=end,direction='forward',scope=scope,mount=mount,connected=True,**exposure())
    ca=composer(channel,2,7,'a','1');cb=composer(O.THREAD_DRAFT,1,6,'b','2')
    keys=[dict(team=O.TEAM,channel=O.CHANNEL,thread=None),dict(team=O.TEAM,channel=O.CHANNEL,thread=O.ROOT)]
    rows=[]
    def source(ts):return dict(type='message',ts=ts,user=O.USER,thread_ts=O.ROOT,text='Known unknown invalid',reply_count=int(ts==O.ROOT),blocks=O.blocks())
    base=dict(contextCurrent=True,canonical=True,account=O.ACCOUNT,epoch=1,team=O.TEAM,user=O.USER,boot=O.boot(),channel=O.CHANNEL,root=O.ROOT,
        channelGeneration=4,channelScope='1-T1-CCHANEMOJI-4',threadScope='thread-original',channelRaw=[source(O.ROOT)],threadRaw=[source(O.ROOT),source(O.REPLY)],
        channelRows=[O.ROOT],threadRows=[O.ROOT,O.REPLY],channelPending=[],threadPending=[],pendingScroll=None,channelWindow=[0,1],threadWindow=[0,2],
        composer=channel,threadComposer=O.THREAD_DRAFT,attachments=[],threadAttachments=[],customEmoji={},customRevision=0,
        drafts=dict(channel=keys[0],thread=keys[1],items=[dict(key=k,value=dict(composer=dict(text=t),attachments=[])) for k,t in zip(keys,[channel,O.THREAD_DRAFT])]))
    def witness(socket,serial,current=True):return dict(current=current,version='v17',response=O.RAW,owner=dict(account=O.ACCOUNT,epoch=1,team=O.TEAM,user=O.USER,socket=socket,slotSerial=serial))
    w1=witness(7,1);w2=witness(8,2)
    inputs=[dict(target='channel',kind='preseeded-occluded',before=dict(value=O.CHANNEL_DRAFT,**exposure())) if narrow else dict(target='channel',kind='synthetic-native-input',before=exposure(),value=channel,isTrusted=False),dict(target='thread',kind='synthetic-native-input',before=exposure(),value=O.THREAD_DRAFT,isTrusted=False)]
    proof=dict(setup=dict(ok=True,inputs=inputs),arm=dict(ok=True,channel=ca,thread=cb))
    for i,name in enumerate(O.STAGES):
        m=copy.deepcopy(base);count=(0,1,1,1,2)[i]
        platform='windows' if sys.platform=='win32' else 'darwin' if sys.platform=='darwin' else 'linux'
        requests=[dict(serial=j+1,socket=7+j,method='POST',path='/api/experiments.getByUser',args=dict(ssb_platform=platform,ssb_release_channel='prod'),tokenPresent=True,factoryMatched=True,response=O.RAW) for j in range(count)]
        witnesses=[] if i==0 else [witness(7,1,i<3)]+([w2] if i==4 else [])
        m.update(socket=7 if i<3 else 8,requests=requests,witnesses=witnesses,selected=witnesses[-1] if i in (1,2,4) else None,
            registered=dict(known=int(i>0),unknown=0,invalid=0),channelProjection=10+i,threadProjection=20+i,mediaEpoch=1 if i<2 else 2)
        if i==1:proof['assignment1']=w1
        if i==2:proof['media']=dict(url=O.URL,id='super-platinum-media://emoji/123',uriBefore='http://super-platinum-media.emoji/123?v=1',epochBefore=1,readyBefore=False,readyAfter=True,mime='image/png',bytes=len((Path(__file__).resolve().parent.parent/'assets/icons/png/icon_16.png').read_bytes()))
        if i==3:proof['retirement']=dict(kind='synthetic-socket-connected',before=w1,after=dict(w1,current=False),socket=8,accessorAvailable=False,networkDispatched=False,workResumed=False)
        if i==4:proof['assignment2']=w2
        ready=i in (2,4);selected=i in (1,2,4)
        glyphs=[dict(text='A',title=':unknown_original:')]
        if not selected:glyphs.insert(0,dict(text='👍🏽',title=':conflicting_original:'))
        elif not ready:glyphs.insert(0,dict(text=':+1::skin-tone-4:',title=':+1::skin-tone-4:'))
        def domrow(ts,j):return dict(id=ts,index=j,height=40,rect=rect,bodyExposure=exposure(),body='Known '+('' if ready else ':+1::skin-tone-4:' if selected else '👍🏽')+' Unknown A Invalid ',glyphs=glyphs,images=[dict(src='http://super-platinum-media.emoji/123?v=2',alt=':+1::skin-tone-4:',title=':+1::skin-tone-4:',complete=True,width=16,height=16,rect=rect)] if ready else [])
        surfaces={k:dict(scope=m[k+'Scope'],generation=str(m[k+'Projection']),rect=rect,scrollTop=0,rows=[domrow(ts,j) for j,ts in enumerate(ids)]) for k,ids in [('channel',[O.ROOT]),('thread',[O.ROOT,O.REPLY])]}
        n=dict(ok=True,viewport=O.expected_viewport(fixture),sameOriginalNodes=True,channelComposer=ca,threadComposer=cb,surfaces=surfaces)
        current=dict(kind='retained-original-DOM-measurement',asyncCallbackHeld=False,sample=surfaces,channelAccepted=True,threadAccepted=True)
        old=None if not i else dict(kind='retained-original-DOM-measurement',asyncCallbackHeld=False,sample=rows[0 if i<3 else i-1]['native']['surfaces'],channelAccepted=False,threadAccepted=False)
        rows.append(copy.deepcopy(dict(ok=True,stage=name,ordinal=i,noSession=False,noTransport=False,isolation=O.ISOLATION,model=m,native=n,proof=proof,measurementGuard=dict(current=current,old=old))))
    return fixture,dict(ok=True,noSession=False,noTransport=False,isolation=O.ISOLATION,stages=rows,layout=rows[-1]['native'],proof=rows[-1]['proof'],model=rows[-1]['model'])

class Rules(unittest.TestCase):
    def reject(self,change):
        fixture,r=sample();change(r)
        with self.assertRaises(RuntimeError):O.validate(r,fixture)
    def test_normal_complete_original_receipt(self):O.validate(*reversed(sample()))
    def test_narrow_retains_occluded_draft(self):O.validate(*reversed(sample(True)))
    def test_no_default_when_unobserved(self):self.reject(lambda r:r['stages'][0]['model'].update(selected=r['stages'][1]['model']['selected']))
    def test_old_map_retired_immediately(self):self.reject(lambda r:r['stages'][3]['model'].update(selected=r['stages'][1]['model']['selected']))
    def test_old_original_handle_not_laundered(self):self.reject(lambda r:r['stages'][4]['model']['witnesses'][0].update(current=True))
    def test_fresh_assignment_new_socket(self):self.reject(lambda r:r['stages'][4]['model']['selected']['owner'].update(socket=7))
    def test_no_extra_assignment_requests(self):self.reject(lambda r:r['stages'][2]['model']['requests'].append(copy.deepcopy(r['stages'][2]['model']['requests'][0])))
    def test_factory_witness_mandatory(self):self.reject(lambda r:r['stages'][1]['model']['requests'][0].update(factoryMatched=False))
    def test_actual_stale_channel_measurement_refused(self):self.reject(lambda r:r['stages'][1]['measurementGuard']['old'].update(channelAccepted=True))
    def test_actual_stale_thread_measurement_refused(self):self.reject(lambda r:r['stages'][3]['measurementGuard']['old'].update(threadAccepted=True))
    def test_old_sample_is_original_receipt(self):self.reject(lambda r:r['stages'][3]['measurementGuard']['old']['sample']['thread'].update(generation='999'))
    def test_no_claim_async_callback_held(self):self.reject(lambda r:r['stages'][1]['measurementGuard']['old'].update(asyncCallbackHeld=True))
    def test_unknown_cannot_register_url(self):self.reject(lambda r:r['stages'][1]['model']['registered'].update(unknown=1))
    def test_no_global_catalog_publication(self):self.reject(lambda r:r['stages'][2]['model']['customEmoji'].update(party='http://external'))
    def test_raw_unicode_priority_preserved(self):self.reject(lambda r:r['stages'][1]['model']['channelRaw'][0]['blocks'][0]['elements'][0]['elements'][1].update(unicode='1f44d'))
    def test_original_thread_raw_not_replaced(self):self.reject(lambda r:r['stages'][2]['model']['threadRaw'][1].update(text='replacement'))
    def test_no_navigation_generation_change(self):self.reject(lambda r:r['stages'][2]['model'].update(channelGeneration=55))
    def test_draft_scope_not_borrowed(self):self.reject(lambda r:r['stages'][2]['native']['threadComposer'].update(scope='a'))
    def test_original_caret_retained(self):self.reject(lambda r:r['stages'][2]['native']['channelComposer'].update(start=0))
    def test_original_media_asset_not_external(self):self.reject(lambda r:r['stages'][2]['native']['surfaces']['channel']['rows'][0]['images'][0].update(src=O.URL))
    def test_invalid_cannot_add_image(self):self.reject(lambda r:r['stages'][0]['native']['surfaces']['thread']['rows'][0]['images'].append({}))
    def test_sessionless_claim_refused(self):self.reject(lambda r:r.update(noSession=True))
    def test_network_work_must_not_resume(self):self.reject(lambda r:r['stages'][3]['proof']['retirement'].update(workResumed=True))
    def test_stages_not_reordered(self):self.reject(lambda r:r['stages'].reverse())
    def test_exact_named_fixture_gate(self):
        p=Path(__file__).resolve().parent.parent/'src/desktop/channel_thread_emoji.rs';text=p.read_text()
        for exact in ['protected-native-fixture','SUPER_PLATINUM_CHANNEL_THREAD_EMOJI_CHECK','channel-thread-emoji-narrow']:self.assertIn(exact,text)
        self.assertIn('Ok("1")',text)
    def test_wrapper_keeps_original_closed_validator(self):
        p=Path(__file__).with_name('check-channel-thread-emoji-native.py');spec=importlib.util.spec_from_file_location('test_channel_thread_wrapper',p);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        self.assertFalse(module.context(module.SPEC,'unrelated',dict(noSession=False,noTransport=False,isolation=O.ISOLATION)))
        self.assertFalse(module.context(module.SPEC,O.PREFIX,dict(noSession=True,noTransport=True,isolation=O.ISOLATION)))

if __name__=='__main__':unittest.main()
