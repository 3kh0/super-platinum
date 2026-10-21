"""Independent rectangular crop math and adversarial original-event receipts."""
import copy
import unittest
import workflow_management_avatar_cropper as C
import workflow_management_first_publish as F
from test_workflow_management_first_publish import server


def rect(x,y,w,h):return dict(left=x,top=y,right=x+w,bottom=y+h,width=w,height=h)


def row(case,stage,epoch=10):
    natural=(640,320) if case==C.WIDE else (256,256)
    dw,dh=C.display_size(*natural);sel=C.expected_selection(case,stage)
    image=dict(id='workflow-finish-crop-image',nodeId=21,connected=True,savedSame=True,rect=rect(100,100,dw,dh),source='http://super-platinum-media.localhost/image?v=1',complete=True,naturalWidth=natural[0],naturalHeight=natural[1])
    selection=dict(sel,id='workflow-finish-crop-selection',nodeId=22,connected=True,savedSame=True,rect=rect(100+sel['x'],100+sel['y'],sel['width'],sel['height']))
    handles=[dict(id='workflow-finish-crop-handle-'+o,nodeId=30+i,connected=True,savedSame=True,ordinal=o,rect=rect(selection['rect']['right']-4,selection['rect']['bottom']-4,8,8)) for i,o in enumerate(C.ORDINALS)]
    cropper=dict(surface=dict(id='workflow-finish-crop-surface',nodeId=20,connected=True,savedSame=True,rect=rect(100,100,dw,dh)),image=image,selection=selection,handles=handles,epoch=epoch,originalNodes=True)
    a=dict(pending=stage=='crop-held',notice=F.CROP_NOTICE if stage=='crop-rejected' else None,generation=9,receipts=[],crop=C.full_crop(sel,natural,dict(width=dw,height=dh)),cropper=dict(ready=True,interactionPending=False,generation=5,originalImage=dict(url='https://example.test/first-publish-upload.png',naturalWidth=natural[0],naturalHeight=natural[1]),display=dict(width=dw,height=dh),selection=copy.deepcopy(sel)))
    c=dict(id=7,visit=2,document=3,revision=4,copy='collaborators',runDraft=dict(mode='everyone'),managerDraft=dict(ids=['UWORKFLOW']),local=dict(epoch=epoch,avatar=a,title=F.LOCAL_TITLE,description=F.LOCAL_DESCRIPTION,iconUrl=''))
    modal=dict(nodeId=10,owner='7',epoch=str(epoch),pane='crop',cropper=cropper,fields=dict(title=dict(nodeId=11,value=F.LOCAL_TITLE),description=dict(nodeId=12,value=F.LOCAL_DESCRIPTION)),upload=dict(nodeId=13,files=[dict(name=str(F.avatar_path(case)),size=0,type=F.DOM_FILE_TYPE)]),custody=dict(file=dict(selectedIdentity=50,savedIdentity=50,sameSaved=True)),details=dict(nodeId=14))
    return dict(stage=stage,model=dict(title=F.TITLE,description=F.DESCRIPTION),proof=dict(firstPublish=dict(confirmation=c),server=server(case,stage),native=dict(scope=dict(visit='2',document='3',revision='4'),fields=dict(title=dict(nodeId=1,value=F.TITLE),description=dict(nodeId=2,value=F.DESCRIPTION)),modal=modal)))


def exercise(case,action):
    prior,current=('uploaded','resized') if action=='resize' else ('resized','dragged') if action=='drag' else ('dragged','crop-held')
    b,a=row(case,prior),row(case,current,11)
    if action=='pending-drag':
        b['proof']['server']=copy.deepcopy(a['proof']['server']);b['proof']['firstPublish']['confirmation']['local']['avatar']['pending']=True
    else:a['proof']['server']=copy.deepcopy(b['proof']['server'])
    before,after=F.observation(b),F.observation(a);bm,am=before['native']['modal'],after['native']['modal']
    bc,ac=bm['cropper'],am['cropper'];sb,sa=bc['selection'],ac['selection']
    target=bc['handles'][4] if action=='resize' else sb
    tr=target['rect'];start=(tr['left']+tr['width']/2,tr['top']+tr['height']/2)
    requested={'size':96} if action=='resize' else {'dx':8,'dy':0} if action=='pending-drag' else {'x':92,'y':46} if case==C.WIDE else {'x':64,'y':64}
    delta=(96-sb['width'],96-sb['height']) if action=='resize' else (8,0) if action=='pending-drag' else (requested['x']-sb['x'],requested['y']-sb['y'])
    events=[dict(type=t,clientX=start[0]+delta[0]*f,clientY=start[1]+delta[1]*f,button=0,buttons=0 if t=='pointerup' else 1,pointerId=41,isTrusted=False,defaultPrevented=t!='pointerup') for t,f in zip(['pointerdown']+['pointermove']*4+['pointerup'],[0,.25,.5,.75,1,1])]
    owner=dict(visit='2',document='3',revision='4',confirmation='7',epoch='10',pane='crop')
    g=dict(ok=True,kind='synthetic-dom-pointer',action=action,requested=requested,sameNodes=True,ownerBefore=owner,ownerAfter=copy.deepcopy(owner),target=dict(id=target['id'],nodeId=target['nodeId'],rect=tr,hit=True),image=copy.deepcopy(bc['image']),selectionBefore=copy.deepcopy(sb),selectionAfter=copy.deepcopy(sa),events=events)
    return dict(before=before,gesture=g,after=after,quiet=dict(before=copy.deepcopy(after),after=copy.deepcopy(after),observedMs=500))


class GeometryTests(unittest.TestCase):
    def test_source_initial_square_and_rectangular_full_image(self):
        self.assertEqual(C.display_size(256,256),(256,256))
        self.assertEqual(C.display_size(640,320),(460,230))
        for natural,wire in [((256,256),'6,6,243'),((640,320),'168,8,304')]:
            w,h=C.display_size(*natural)
            self.assertEqual(C.wire_crop(C.full_crop(C.initial_selection(w,h),natural,dict(width=w,height=h))),wire)

    def test_math_round_ties_not_bankers(self):
        self.assertEqual(C.js_round(128.5),129)
        self.assertEqual(C.js_round(178.08695652173913),178)
        for v in (-1,float('nan'),float('inf'),True):
            with self.subTest(v=v),self.assertRaises(RuntimeError):C.js_round(v)

    def test_no_local_dimension_substitution_or_unsquared_crop(self):
        for change in ({'x':400},{'width':129},{'height':0}):
            s=dict(x=92,y=46,width=128,height=128);s.update(change)
            with self.subTest(change=change),self.assertRaises(RuntimeError):C.full_crop(s,(640,320),dict(width=460,height=230))

    def test_final_vectors(self):
        self.assertEqual(C.wire_crop(C.full_crop(dict(x=92,y=46,width=128,height=128),(640,320),dict(width=460,height=230))),'128,64,178')
        self.assertEqual(C.wire_crop(C.full_crop(dict(x=64,y=64,width=128,height=128),(256,256),dict(width=256,height=256))),'64,64,128')

    def test_all_active_stage_snapshots(self):
        for case in C.STAGES:
            for stage in C.STAGES[case]:
                if stage in C.CROP_STAGES:
                    with self.subTest(case=case,stage=stage):C.snapshot(case,row(case,stage))

    def test_wrong_reply_dimensions_scale_and_full_tuple_rejected(self):
        for key in ('natural','display','tuple','selection','handle','epoch','ready'):
            r=row(C.WIDE,'dragged');a=r['proof']['firstPublish']['confirmation']['local']['avatar'];d=r['proof']['native']['modal']['cropper']
            if key=='natural':a['cropper']['originalImage']['naturalWidth']=256
            elif key=='display':a['cropper']['display']['width']=640
            elif key=='tuple':a['crop']['size']=128
            elif key=='selection':d['selection']['x']+=1
            elif key=='handle':d['handles'][0]['savedSame']=False
            elif key=='epoch':d['epoch']=9
            else:a['cropper']['ready']=False
            with self.subTest(key=key),self.assertRaises(RuntimeError):C.snapshot(C.WIDE,r)


class GestureTests(unittest.TestCase):
    def test_layout_quantization_preserves_exact_event_model_math(self):
        e=exercise('first-avatar-crop-drag','drag')
        # Native layout quantizes 6.4 to a 1/64 CSS-pixel boundary. The fixture
        # endpoint subtracts the measured coordinate, not the model decimal.
        sb=e['gesture']['selectionBefore'];dx=6.390625-sb['x'];dy=6.390625-sb['y']
        for point in (sb,e['before']['native']['modal']['cropper']['selection']):
            point['x']+=dx;point['y']+=dy
            point['rect']['left']+=dx;point['rect']['right']+=dx
            point['rect']['top']+=dy;point['rect']['bottom']+=dy
        e['gesture']['target']['rect']=copy.deepcopy(sb['rect'])
        for event,fraction in zip(e['gesture']['events'],[0,.25,.5,.75,1,1]):
            event['clientX']+=dx*(1-fraction);event['clientY']+=dy*(1-fraction)
        a=e['after']['firstPublish']['confirmation']['local']['avatar']
        for point in (a['cropper']['selection'],e['after']['native']['modal']['cropper']['selection'],e['gesture']['selectionAfter']):
            point['x']-=dx;point['y']-=dy
            if 'rect' in point:
                point['rect']['left']-=dx;point['rect']['right']-=dx
                point['rect']['top']-=dy;point['rect']['bottom']-=dy
        a['crop']['x']-=dx;a['crop']['y']-=dy
        e['quiet']['before']=copy.deepcopy(e['after']);e['quiet']['after']=copy.deepcopy(e['after'])
        C.gesture('first-avatar-crop-drag',e,'drag')
        self.assertEqual(C.wire_crop(a['crop']),'64,64,128')
        a['cropper']['selection']['x']=64
        with self.assertRaises(RuntimeError):C.gesture('first-avatar-crop-drag',e,'drag')

    def test_held_late_global_receipt_does_not_adopt_old_crop(self):
        for case in C.HELD:
            s=server(case,'late-settled')
            upload,crop,reupload=[r for r in s['requests'] if r['phase']=='avatar']
            # This is actual settlement order: second chooser completes while
            # the original crop sender remains held, then original crop settles.
            s['settlements']=[x for x in s['settlements'] if x['serial']!=crop['serial']]+[dict(serial=crop['serial'],returned=True)]
            current_id=7 if case.endswith('back-held') else 8
            receipts=[]
            for request,id,generation in ((upload,7,2),(reupload,current_id,5),(crop,7,3)):
                receipts.append(dict(id=id,generation=generation,failed=False,request=copy.deepcopy(request['request']),raw=copy.deepcopy(s['responses'][request['serial']-1]['result']['raw'])))
            local=[dict(kind='upload',raw=copy.deepcopy(r['raw'])) for r in receipts[:-1] if r['id']==current_id]
            r=dict(stage='late-settled',proof=dict(server=s,firstPublish=dict(avatarReceipts=receipts,confirmation=dict(id=current_id,local=dict(avatar=dict(receipts=local))))))
            F.server_proof(case,'late-settled',s);C.avatar_receipts(case,r)
            for mutation in ('adopt','lost','raw','request','failed'):
                bad=copy.deepcopy(r);p=bad['proof']['firstPublish']
                if mutation=='adopt':p['confirmation']['local']['avatar']['receipts'].append(dict(kind='crop',raw=copy.deepcopy(receipts[-1]['raw'])))
                elif mutation=='lost':p['avatarReceipts'].pop()
                elif mutation=='raw':p['avatarReceipts'][-1]['raw']['url']='https://foreign.test/crop.png'
                elif mutation=='request':p['avatarReceipts'][-1]['request']['fields'][1][1]='72,64,128'
                else:p['avatarReceipts'][-1]['failed']=True
                with self.subTest(case=case,mutation=mutation),self.assertRaises(RuntimeError):C.avatar_receipts(case,bad)

    def test_actual_original_synthetic_sequences(self):
        for case in ('first-avatar-crop-drag',C.WIDE):
            for action in ('resize','drag'):
                with self.subTest(case=case,action=action):C.gesture(case,exercise(case,action),action)

    def test_event_mutations_rejected(self):
        for key,value in [('isTrusted',True),('pointerId',42),('buttons',0),('type','mousemove'),('clientX',-1)]:
            e=exercise(C.WIDE,'resize');e['gesture']['events'][1][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):C.gesture(C.WIDE,e,'resize')
        e=exercise(C.WIDE,'resize');e['gesture']['events'].pop()
        with self.assertRaises(RuntimeError):C.gesture(C.WIDE,e,'resize')

    def test_original_owner_nodes_file_and_hit_mutations_rejected(self):
        for key in ('owner','file','handle','image','hit','requested','wire','quiet'):
            e=exercise(C.WIDE,'resize')
            if key=='owner':e['gesture']['ownerAfter']['confirmation']='99'
            elif key=='file':e['after']['native']['modal']['custody']['file']['selectedIdentity']=99
            elif key=='handle':e['after']['native']['modal']['cropper']['handles'][0]['nodeId']=99
            elif key=='image':e['gesture']['image']['source']='https://foreign.test/image'
            elif key=='hit':e['gesture']['target']['hit']=False
            elif key=='requested':e['gesture']['requested']['size']=128
            elif key=='wire':e['after']['server']['requests'].append({})
            else:e['quiet']['after']['model']['title']='changed'
            with self.subTest(key=key),self.assertRaises(RuntimeError):C.gesture(C.WIDE,e,'resize')

    def test_pending_edit_cannot_rewrite_operation_generation(self):
        C.gesture('first-avatar-crop-back-held',exercise('first-avatar-crop-back-held','pending-drag'),'pending-drag')
        e=exercise('first-avatar-crop-back-held','pending-drag');e['after']['firstPublish']['confirmation']['local']['avatar']['generation']+=1
        with self.assertRaises(RuntimeError):C.gesture('first-avatar-crop-back-held',e,'pending-drag')

    def test_all_closed_case_stage_ledger_shapes(self):
        for case,stages in C.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):F.server_proof(case,stage,server(case,stage))

    def test_independent_crop_wire_and_raw_rejection(self):
        for key in ('cropbox','raw','applied'):
            s=server(C.WIDE,'crop-rejected')
            if key=='cropbox':s['requests'][-1]['request']['fields'][1][1]='92,46,128'
            elif key=='raw':s['responses'][-1]['result']['raw']['error']='other'
            else:s['requests'][-1]['applied']=True
            with self.subTest(key=key),self.assertRaises(RuntimeError):F.server_proof(C.WIDE,'crop-rejected',s)

    def test_case_specific_real_chooser_asset(self):
        request=dict(fixture='workflow-editor',case=C.WIDE,pid=12,serial=1,input='workflow-finish-upload',path=str(C.WIDE_PATH))
        F.chooser_request(request,C.WIDE,'workflow-editor',12,1)
        request['path']=str(F.AVATAR_PATH)
        with self.assertRaises(RuntimeError):F.chooser_request(request,C.WIDE,'workflow-editor',12,1)
        self.assertEqual(F.chooser_count('first-avatar-crop-back-held'),2)


if __name__=='__main__':unittest.main()
