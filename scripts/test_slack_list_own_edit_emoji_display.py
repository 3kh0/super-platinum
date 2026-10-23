"""Focused independent receipt mutations; no native execution."""
import copy
import json
import runpy
import unittest
from pathlib import Path
import slack_list_own_edit_emoji_display as V

OWNER={'account':'00000000-0000-4000-8000-000000000001','epoch':3}
MOUNT={'scope':'list-edit:1:2','generation':'2','mount':'original-mount'}
def ticket(ops):return dict(ops=copy.deepcopy(ops),mode='PlainText',revision=3,generation='2',composing=False,pending=False,attachmentEffects='none-v1')
def source(count=2,recovery=False):
    return dict(networkCalls=0,httpCapability=False,contextsCurrent=[True]*count,requests=[dict(serial=i,request=dict(method='POST',path='/api/emoji.list',args={}),owner=dict(OWNER,team='T1',user=V.O['SELF']),response=dict(error='offline') if recovery and i==1 else dict(raw=V.RAW,bytes=V.RAW_BYTES),settled=True,contextCurrentAtReturn=True) for i in range(1,count+1)])
def native(ops,action='state'):
    return dict(ok=True,action=action,scope=MOUNT['scope'],generation='2',sameNode=True,delta=ticket(ops),selection=dict(contained=True,collapsed=True,anchorOffset=1,focusOffset=1,text=''),embeds=[dict(label=o['insert']['slackemoji']['text'],text='',image=dict(src='http://super-platinum-media.localhost/emoji/abc?v=1',alt=':parrot_alias:',complete=True,width=1,height=1) if o['insert']['slackemoji']['text']==':parrot_alias:' else None) for o in ops if isinstance(o['insert'],dict)])
def media():
    before=native(V.INITIAL_OPS,'media-hold');after=native(V.INITIAL_OPS,'media-ready');a=before['embeds'][1]['image'];b=after['embeds'][1]['image'];b.update(src=a['src'].replace('v=1','v=2'),width=16,height=16)
    after.update(sameSelection=True,sameDelta=True,sameImage=True,oldSrc=a['src'],newSrc=b['src'])
    delivery=dict(slot=f"list-comment/{OWNER['account']}/T1/parrot_alias",url=V.RAW['emoji']['party_parrot'],id='super-platinum-media://emoji/abc',uriBefore=a['src'],epochBefore=1,readyBefore=False,readyAfter=True,mime='image/png',bytes=(V.ROOT/'assets/icons/png/icon_16.png').stat().st_size)
    return dict(mediaBefore=before,mediaAfter=after,mediaDelivery=delivery,originalDisk={'account':OWNER['account']})

def active_model(revision=0,saving=False,expected=None):
    a=dict(current=True,scope=MOUNT['scope'],generation=2,ts=V.O['TS'],revision=revision,blocks=[],source=V.INITIAL['messages'][1],preserved=V.INITIAL['messages'][1]['blocks'][1:],busy=saving,nativeReady=True,captureFailed=False,composing=False,nativePending=False,plainDelta=dict(version=1,ops=expected or V.INITIAL_OPS),attempt=7 if saving else None,notice=V.T['NOTICES'][9] if saving else None)
    return dict(edit=dict(active=a,opening=False,events=0,retired=[],attempts=[{'id':7}] if saving else []),capture=dict(scope=MOUNT['scope'],generation=2,mount=MOUNT['mount'],failed=False,canSave=not saving,canCancel=not saving,canCheck=False,canRetry=False,busy=saving,finishing=False))

def transcript(accepted=False):
    thumb='\U0001f44d'+('\U0001f3fe' if accepted else '\U0001f3fd');glyphs=[thumb,':parrot_alias:']+(['\U0001f604','\U0001f604'] if accepted else []);titles=[':thumbsup:',':parrot_alias:']+([':smile:',':smile:'] if accepted else [])
    text=f'Initial {thumb} Custom :parrot_alias:'+(' Typed \U0001f604 \U0001f604' if accepted else '')+'Preserved original context'
    v=dict(ok=True,action='transcript',ts=V.O['TS'],pane=1,row=2,body=3,bodyText=text,renderedText=text,images=0,viewport=[1000,800],emojis=[dict(node=i+4,text=t,rendered=t,title=n,codepoints=[ord(c) for c in t],connected=True,painted=True,rect={}) for i,(t,n) in enumerate(zip(glyphs,titles))])
    return dict(before=v,after=copy.deepcopy(v)),dict(viewport=[1000,800],row={'text':'Author'+text+'Edit'})

class DisplayTests(unittest.TestCase):
    def reject(self,fn,*args):
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError,IndexError)):fn(*args)
    def test_authored_initial_and_future_remain_independent_exact_bytes(self):
        for name in V.HASHES:self.assertEqual(V.authored(name).encode(),(V.DATA/name).read_bytes())
        self.assertNotEqual(V.INITIAL_BYTES,V.FUTURE_BYTES)
        a=V.INITIAL['messages'][1];b=V.FUTURE['messages'][1]
        self.assertEqual(a['blocks'][1:],b['blocks'][1:]);self.assertEqual(a['reactions'],b['reactions']);self.assertEqual(a['edit_fixture_opaque'],b['edit_fixture_opaque'])
    def test_standard_encoding_has_unicode_not_custom_skin_tone_field(self):
        leaf=V.BODY[0]['elements'][0]['elements'][1]
        self.assertEqual(leaf,dict(type='emoji',name='thumbsup',unicode='1f44d-1f3fe'))
        self.assertEqual(V.FUTURE['messages'][1]['text'],V.TEXT)
    def test_literal_named_and_unicode_are_two_distinct_semantic_occurrences(self):
        self.assertEqual([o['insert']['slackemoji']['text'] for o in V.CHANGED_OPS if isinstance(o['insert'],dict)],[':thumbsup::skin-tone-5:',':parrot_alias:',':smile:',':smile:'])
    def test_complete_success_and_failure_retry_sources(self):
        for r in (False,True):V.catalog(source(recovery=r),OWNER,2,r)
    def test_catalog_cannot_invent_complete_empty_or_alias_target(self):
        for raw in ({'ok':True,'emoji':{}},{'ok':True,'emoji':{'parrot_alias':V.RAW['emoji']['party_parrot']}}):
            v=source();v['requests'][0]['response']['raw']=raw;self.reject(V.catalog,v,OWNER,2,False)
    def test_catalog_exact_authored_bytes_not_reencoded_receipt(self):
        v=source();v['requests'][0]['response']['bytes']=json.dumps(V.RAW,indent=2);self.reject(V.catalog,v,OWNER,2,False)
    def test_wrong_context_epoch_currentness_factory_refused(self):
        for key in ('epoch','current','factory'):
            v=source()
            if key=='epoch':v['requests'][0]['owner']['epoch']+=1
            if key=='current':v['contextsCurrent'][0]=False
            if key=='factory':v['requests'][0]['request']['args']={'names':'parrot_alias'}
            self.reject(V.catalog,v,OWNER,2,False)
    def test_missing_or_additional_catalog_read_refused(self):
        self.reject(V.catalog,source(1),OWNER,2,False);self.reject(V.catalog,source(3),OWNER,2,False)
    def test_actual_display_receipt_and_foreign_generation(self):
        n=native(V.INITIAL_OPS);V.native(n,V.INITIAL_OPS,MOUNT);n['generation']='foreign';self.reject(V.native,n,V.INITIAL_OPS,MOUNT)
    def test_semantic_emoji_must_not_flatten_to_text(self):
        n=native(V.INITIAL_OPS);n['delta']['ops']=[{'insert':':thumbsup: :parrot_alias:\n'}];self.reject(V.native,n,V.INITIAL_OPS,MOUNT)
    def test_custom_url_never_enters_dom_witness(self):
        n=native(V.INITIAL_OPS);n['embeds'][1]['image']['src']=V.RAW['emoji']['party_parrot'];self.reject(V.native,n,V.INITIAL_OPS,MOUNT)
    def test_unknown_pending_or_missing_capability_ticket_refused(self):
        for key,value in [('pending',True),('composing',True),('attachmentEffects',None)]:
            n=native(V.INITIAL_OPS);n['delta'][key]=value;self.reject(V.native,n,V.INITIAL_OPS,MOUNT)
    def test_actual_media_epoch_only_custody(self):V.media(media(),MOUNT)
    def test_initial_model_zero_is_distinct_from_positive_native_ticket(self):
        m=active_model();V.active(m,MOUNT,V.INITIAL_OPS,'display-pending',[]);V.active(m,MOUNT,V.INITIAL_OPS,'display-ready',[])
        m['edit']['active']['revision']=1;self.reject(V.active,m,MOUNT,V.INITIAL_OPS,'display-ready',[])
    def test_mutated_model_must_advance_but_save_cannot_change_revision(self):
        m=active_model(expected=V.ops(4,True));self.reject(V.active,m,MOUNT,V.ops(4,True),'typed',[])
        typed=active_model(2,expected=V.ops(4,True));V.active(typed,MOUNT,V.ops(4,True),'typed',[])
        rows=[{'stage':'typed','model':typed},{'stage':'tone','model':active_model(5,expected=V.CHANGED_OPS)}]
        self.reject(V.active,active_model(2,expected=V.CHANGED_OPS),MOUNT,V.CHANGED_OPS,'tone',rows)
        V.active(active_model(5,True,V.CHANGED_OPS),MOUNT,V.CHANGED_OPS,'saving',rows)
        self.reject(V.active,active_model(6,True,V.CHANGED_OPS),MOUNT,V.CHANGED_OPS,'saving',rows)
    def test_dirty_epoch_may_advance_after_original_image_capture(self):
        p=media();p['mediaDelivery'].update(epochBefore=2,uriBefore=p['mediaDelivery']['uriBefore'].replace('v=1','v=2'));p['mediaAfter']['embeds'][1]['image']['src']=p['mediaAfter']['embeds'][1]['image']['src'].replace('v=2','v=3');p['mediaAfter']['newSrc']=p['mediaAfter']['embeds'][1]['image']['src'];V.media(p,MOUNT)
    def test_image_ready_epoch_cannot_precede_delivery_or_borrow_asset(self):
        p=media();p['mediaDelivery'].update(epochBefore=2,uriBefore=p['mediaDelivery']['uriBefore'].replace('v=1','v=2'));self.reject(V.media,p,MOUNT)
        p=media();p['mediaDelivery']['uriBefore']=p['mediaDelivery']['uriBefore'].replace('/abc','/other');self.reject(V.media,p,MOUNT)
    def test_media_revision_caret_nodes_and_opaque_identity_cannot_change(self):
        for key in ('revision','caret','node','id'):
            p=media()
            if key=='revision':p['mediaAfter']['delta']['revision']+=1
            if key=='caret':p['mediaAfter']['selection']['anchorOffset']+=1
            if key=='node':p['mediaAfter']['sameImage']=False
            if key=='id':p['mediaAfter']['embeds'][1]['image']['src']='http://super-platinum-media.localhost/emoji/foreign?v=2'
            self.reject(V.media,p,MOUNT)
    def test_media_must_be_original_pending_then_actual_fixture_png(self):
        for key in ('pending','ready','slot','bytes'):
            p=media()
            if key=='pending':p['mediaBefore']['embeds'][1]['image']['width']=16
            if key=='ready':p['mediaAfter']['embeds'][1]['image']['complete']=False
            if key=='slot':p['mediaDelivery']['slot']='other-account'
            if key=='bytes':p['mediaDelivery']['bytes']=0
            self.reject(V.media,p,MOUNT)
    def test_full_ordered_page_preserves_exact_raw_without_style_fabrication(self):
        p=dict(raw=V.INITIAL,messages=V.INITIAL['messages'],witness=dict(messageIndex=1,raw=V.INITIAL['messages'][1],styleKeys=None));V.page(p,V.INITIAL_BYTES)
        p=copy.deepcopy(p);p['witness']['styleKeys']=[];self.reject(V.page,p,V.INITIAL_BYTES)
    def test_wrapper_admits_only_named_cases_and_preserves_old_sources(self):
        paths=[Path('scripts/check-list-own-edit-plain-native.py'),Path('scripts/slack_list_own_edit_plain.py')];before=[p.read_bytes() for p in paths]
        h=runpy.run_path('scripts/check-list-own-edit-emoji-display-native.py')['harness']()
        for c in V.CASES:self.assertEqual(h['case_contract'](c)[0],V.STAGES[c])
        self.assertEqual(before,[p.read_bytes() for p in paths])
    def test_protected_fixture_and_complete_caller_dispatch(self):
        p=V.ROOT/'src/desktop/slack_lists';s=(p/'comments/fixture/plain/emoji_display.rs').read_text()
        for gate in ('protected-native-fixture','compose::check_enabled()','SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK','SUPER_PLATINUM_LIST_OWN_EDIT_EMOJI_DISPLAY_CHECK'):self.assertIn(gate,s)
        for name in V.CASES:self.assertIn(name,(p/'check/compose_comments.rs').read_text())
        self.assertIn('return emoji_display::run',(p/'check/compose_comments/own_edit/plain.rs').read_text())
    def test_no_runtime_semantic_model_or_media_refresh_injection(self):
        s=(V.ROOT/'src/desktop/slack_lists/check/compose_comments/own_edit/plain/emoji_display.js').read_text()
        for forbidden in ('setContents','setDelta','updateContents','refreshMedia','history.cutoff'):self.assertNotIn(forbidden,s)
        self.assertIn("execCommand('insertText'",s);self.assertIn("MouseEvent('click'",s)
    def test_shared_transcript_initial_and_saved_exact_codepoints(self):
        for saved in (False,True):p,g=transcript(saved);V.transcript(p,saved,g)
    def test_shared_transcript_cannot_use_base_tone_or_private_editor_proof(self):
        p,g=transcript();p['before']['emojis'][0].update(text='\U0001f44d',rendered='\U0001f44d',codepoints=[0x1f44d]);self.reject(V.transcript,p,False,g)
        p,g=transcript();p['before']['action']='state';self.reject(V.transcript,p,False,g)
    def test_shared_node_owner_or_title_cannot_change_across_capture(self):
        for which in ('row','node','title'):
            p,g=transcript(True)
            if which=='row':p['after']['row']=99
            if which=='node':p['after']['emojis'][0]['node']=99
            if which=='title':p['after']['emojis'][0]['title']=':thumbsup::skin-tone-5:'
            self.reject(V.transcript,p,True,g)
    def test_shared_custom_literal_does_not_claim_image_catalog_publication(self):
        p,g=transcript(True);p['before']['images']=1;self.reject(V.transcript,p,True,g)
        p,g=transcript(True);p['before']['emojis'][1]['text']='custom image';self.reject(V.transcript,p,True,g)
    def test_shared_hidden_body_or_wrong_saved_glyph_refused(self):
        p,g=transcript();p['before']['emojis'][0]['painted']=False;self.reject(V.transcript,p,False,g)
        p,g=transcript(False);self.reject(V.transcript,p,True,g)

if __name__=='__main__':unittest.main()
