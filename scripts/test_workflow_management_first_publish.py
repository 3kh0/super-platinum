"""Constructed closed receipts and adversarial mutations; no native launches."""
import copy
import unittest
import workflow_management_first_publish as F


def read_args(method,initial_resolution=False):
    fixed={
        'experiments.getByUser':dict(ssb_platform='windows',ssb_release_channel='prod'),
        'client.init':dict(include_relevant_onboarding='false',_x_reason='native-workflow-capabilities'),
        'features.access.policies.list':dict(_x_reason='native-workflow-capabilities'),
        'client.extras':dict(extras='["team_prefs"]',_x_reason='native-workflow-capabilities'),
        F.DISCOVERY:dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url'),
        F.FACTS[0]:dict(_x_reason='fetch-connector-apps'),F.FACTS[1]:dict(_x_reason='fetching-workflow-trigger-types-list-wfb'),
    }
    if method in fixed:return fixed[method]
    if method=='workflows.triggers.list':return dict(app_ids='',_x_reason='automations_gallery') if initial_resolution else dict(app_id='A_HOME',_x_reason='load-workflow-editor')
    key,value=('trigger_id','TR_REVIEW') if method in ('workflows.triggers.info','workflows.triggers.permissions.list') else ('workflow_id','WfREVIEW01')
    return {key:value,'_x_reason':'load-workflow-editor'}


def server(case,stage,late_managers=False):
    spec=F.stage_state(case,stage);original=F.original(case);current=copy.deepcopy(original)
    result=dict(owner=copy.deepcopy(F.OWNER),ownerCurrent=not spec['retired'],networkCalls=0,
        original=original,current=current,seed=dict(boot=dict(team=dict(id='TWORKFLOW',name='Home fixture'),users=[dict(id='UWORKFLOW'),dict(id='UHOME0001')]+([dict(id='UMANOLD01')] if F.settings(case) else [])),
            init=dict(team=dict(id='TWORKFLOW',name='Home fixture')),canonicalTeams={'TWORKFLOW':dict(id='TWORKFLOW',name='Home fixture')}),requests=[],responses=[],settlements=[],releases=[],held=[])
    def add(phase,method,raw,fields=None,**extra):
        n=len(result['requests'])+1
        row=dict(serial=n,phase=phase,method=method,owner={k:F.OWNER[k] for k in ('account','epoch','team')},applied=phase in ('mutation','avatar'))
        if fields is not None:row['fields']=fields
        row.update(extra);result['requests'].append(row);result['responses'].append(dict(serial=n,result=copy.deepcopy(raw)))
    def read(method,resolution=False):
        if method=='users/search':
            body=dict(query='manager',count=24,fuzz=1,enable_workspace_ranking=True,filter='NOT deactivated AND NOT guests',include_profile_only_users=True)
            add('picker',method,{'raw':dict(ok=True,results=[F.person('UMANNEW01','New Manager')],next_marker='')},kind='Members',query=body,body=body,retrySafe=False);return
        if method==F.FACTS[0]:raw=F.CERTIFIED
        elif method==F.FACTS[1]:raw=F.TYPES
        elif method==F.DISCOVERY:raw=F.DISCOVERY_RAW
        elif method=='functions.workflows.getLatest':
            w=copy.deepcopy(current['workflow'])
            if case=='first-stale-readback' and w['is_published']:w['is_published']=False
            raw=F.latest(w)
        elif method=='workflows.triggers.info':raw=dict(ok=True,trigger=current['trigger'])
        elif method=='workflows.triggers.list':raw=dict(ok=True,triggers=[current['trigger']])
        elif method=='functions.workflows.permissions.copy.list':raw=current['copy']
        elif method=='workflows.triggers.permissions.list':raw=current['run']
        else:raw=F.read_result(method)['raw']
        add('read',method,{'raw':raw},list(read_args(method,resolution).items()))
    def mutate(method):
        raw=F.mutation_result(case,method,current);fields=F.mutation_fields(case,method)
        add('mutation',method,raw,fields,reason=dict(fields)['_x_reason'],applied=raw.get('raw',{}).get('ok') is not False)
    def avatar(crop=False,rejected=False):
        path=F.avatar_path(case)
        fields=dict(avatar='AVATAR_FIRST',cropbox=F.C.expected_wire(case) if case in F.C.STAGES else '64,64,128',_x_reason='workflows_crop_workflow_avatar') if crop else dict(_x_reason='workflows_upload_workflow_avatar')
        image=None if crop else dict(filename=path.name,mime='image/png',width=640 if case==F.C.WIDE else 256,height=320 if case==F.C.WIDE else 256,length=path.stat().st_size)
        raw=dict(ok=True,url='https://example.test/first-publish-crop.png',opaque_receipt='original-crop') if crop else dict(ok=True,id='AVATAR_FIRST',url='https://example.test/first-publish-upload.png',opaque_receipt='original-upload')
        if rejected:raw=dict(ok=False,error='invalid_crop' if crop else 'invalid_image',opaque_receipt='original-crop-rejected' if crop else 'original-upload-rejected')
        add('avatar','avatars.crop' if crop else 'avatars.upload',{'raw':raw},request=dict(tokenPresent=True,fields=list(fields.items()),image=image),applied=not rejected)
    if stage!='about':
        for i,m in enumerate(F.INITIAL):read(m,i==4)
    if F.settings(case) and stage not in ('about','editor','draft','finish'):
        read(F.DISCOVERY);read('users/search')
    count=F.avatar_count(case,stage)
    if count:
        avatar(False,case in ('first-avatar-upload-back','first-avatar-upload-retry'))
        if case in F.C.STAGES:
            if count>=2:avatar(True,case==F.C.WIDE)
            if count>=3:avatar(case==F.C.WIDE)
        elif case=='first-avatar-upload-retry':
            if count>=2:avatar()
            if count>=3:avatar(True)
        elif case in F.PANE_CASES:pass
        else:
            if count>=2:avatar(True,case!='first-avatar')
            if count>=3:avatar(True)
    if spec['sent']:
        for m in F.POLICY+F.EDITOR+F.FACTS:read(m)
        if case!='first-default':mutate(F.UPDATE);mutate(F.TRIGGER_UPDATE)
        if F.settings(case):
            mutate(F.COPY);mutate(F.RUN)
            if not late_managers:mutate(F.ADD);mutate(F.REMOVE)
            if case=='first-settings':mutate(F.RESTRICTION)
        mutate(F.PUBLISH)
        if F.settings(case) and late_managers:mutate(F.ADD);mutate(F.REMOVE)
        if case=='first-settings-rejected-held' and stage!='published-while-held':mutate(F.RESTRICTION)
    if spec['checked']:
        for m in F.POLICY+F.EDITOR:read(m)
        if stage=='checked-again':
            for m in F.POLICY+F.EDITOR:read(m)
    held_method=F.RUN if case=='first-settings-rejected-held' else 'avatars.upload' if case=='first-avatar-upload-cancel-held' else 'avatars.crop' if case in F.C.HELD else F.PUBLISH
    held=next((r['serial'] for r in result['requests'] if r['method']==held_method),None)
    result['held']=[held] if spec['held'] else []
    result['settlements']=[dict(serial=r['serial'],returned=True) for r in result['requests'] if r['serial'] not in result['held']]
    if case=='first-settings-rejected-held' and stage in ('confirmed','checked','done') or stage=='late-settled':result['releases']=[dict(serial=held,receiverRetired=False,delivered=True)]
    # JSON serialization makes original tuple form rows into actual receipt arrays.
    return F.json.loads(F.json.dumps(result))


def attempt(case,stage,s):
    spec=F.stage_state(case,stage);returned={r['serial'] for r in s['settlements']};slots={}
    for row in s['requests']:
        if row['phase']!='mutation':continue
        reply=s['responses'][row['serial']-1]['result'];raw=reply.get('raw')
        outcome='in-flight' if row['serial'] not in returned else 'unknown' if 'error'in reply else 'rejected' if raw['ok'] is False else 'acknowledged'
        args=dict(F.mutation_fields(case,row['method']));args.pop('_x_reason')
        for key in ('inputs','shortcut'):
            if key in args:args[key]=F.json.loads(args[key])
        if 'collaborator_ids' in args:args['collaborator_ids']=args['collaborator_ids'].split(',')
        slots[row['method']]=dict(method=row['method'],arguments=args,outcome=outcome,raw=raw if outcome!='in-flight' else None,failure='delivery unconfirmed' if outcome=='unknown' else None)
    if not spec['sent']:return dict(attempts=[])
    pub=slots[F.PUBLISH];complete=not spec['held'] and pub['outcome']=='acknowledged'
    a=dict(id=1,originalTitle=F.original(case)['workflow']['title'] if case=='first-default' else F.TITLE,
        originalDescription=F.original(case)['workflow']['description'] if case=='first-default' else F.DESCRIPTION,
        validation=dict(status='ready',certifiedApps=F.CERTIFIED,triggerTypes=F.TYPES,assessment=dict(blockingErrors=[],hasSteps=True,publishRestricted=False,canBePublished=True,unavailableSteps=[],triggerAvailable=True)),
        metadata=[slots[F.UPDATE],slots[F.TRIGGER_UPDATE]] if F.UPDATE in slots else None,copy=slots.get(F.COPY),
        run=dict(primary=slots[F.RUN],followup=dict(slots=[slots[F.RESTRICTION]]) if F.RESTRICTION in slots else None) if F.RUN in slots else None,
        managers=dict(slots=[slots[F.ADD],slots[F.REMOVE]]) if F.ADD in slots else None,
        publication=pub,followup=dict(stages=[],fullySucceeded=True) if pub['outcome']=='acknowledged' else None,
        finished=not spec['held'],complete=complete,resolved=False,readback=None,
        continuation='metadata-rejected' if case=='first-metadata-rejected' else 'settings-rejected' if case=='first-settings-rejected-held' else 'settings-complete')
    if spec['checked']:
        w=copy.deepcopy(s['current']['workflow'])
        if case=='first-stale-readback':w['is_published']=False
        matched=bool(pub['raw'] and F.exact(pub['raw']['decorated_workflow'],w))
        a.update(readback=dict(workflow=w,trigger=s['current']['trigger'],matches=matched),resolved=complete and matched)
    return dict(attempts=[a])


def sample_pane(n,stage):
    m=n['modal'];pane=F.expected_pane(stage);details=pane=='details'
    m.setdefault('epoch','19');m.update(pane=pane,details=dict(id='workflow-finish-details',nodeId=90,connected=True,displayed=details,savedSame=True),
        activePane=dict(id='workflow-finish-details' if details else 'workflow-finish-'+pane+'-pane',nodeId=90 if details else 91,connected=True,displayed=True,savedSame=True,bindingEpoch=m['epoch']))
    for i,k in enumerate(('title','description')):m['fields'][k].update(nodeId=m['fields'][k].get('nodeId',30+i),connected=True,displayed=details,readonly=not details)
    m['upload'].update(nodeId=m['upload'].get('nodeId',32),connected=True,displayed=details,disabled=not details)
    def paint(id,text):return dict(id=id,text=text,nodeId=100+len(id),visible=True,hit=True,disabled=False,rect=dict(left=10,top=10,right=110,bottom=40,width=100,height=30))
    m['inactiveControls']=[]
    if details:n['controls']=[paint('workflow-finish-submit','Next'),paint('workflow-finish-cancel','Cancel')];return
    for key in ('title','description','upload','cancel','submit'):
        v=m['fields'].get(key,m.get(key,{}))
        m['inactiveControls'].append(dict(id='workflow-finish-'+key,nodeId=v.get('nodeId',80+len(key)),value=v.get('value'),connected=True,displayed=False))
    names={'workflow-finish-upload-back':'Back','workflow-finish-upload-cancel':'Cancel'} if pane=='upload' else {'workflow-finish-crop-back':'Back','workflow-finish-crop-apply':'Save','workflow-finish-crop-cancel':'Cancel',**{'workflow-finish-crop-'+k:'' for k in ('x','y','size')}}
    if stage=='upload-rejected':names['workflow-finish-upload-retry']='Try a Different Image'
    m['heading']='Crop your photo' if pane=='crop' else 'Eek' if stage=='upload-rejected' else 'Uploading your photo\u2026'
    n['controls']=[paint(k,v) for k,v in names.items()]


class LedgerTests(unittest.TestCase):
    def check(self,case,stage,value=None):
        s=server(case,stage) if value is None else value
        return F.server_proof(case,stage,s)

    def test_all_case_stage_ledgers(self):
        for case,stages in F.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):self.check(case,stage)

    def test_all_actual_slot_shapes(self):
        for case,stages in F.STAGES.items():
            for stage in stages:
                with self.subTest(case=case,stage=stage):
                    s=server(case,stage);F.attempt_proof(case,stage,attempt(case,stage,s),s,F.stage_state(case,stage))

    def test_metadata_rejected_still_publishes(self):
        s=server('first-metadata-rejected','checked');methods=[r['method'] for r in s['requests'] if r['phase']=='mutation']
        self.assertEqual(methods,[F.UPDATE,F.TRIGGER_UPDATE,F.PUBLISH]);self.check('first-metadata-rejected','checked',s)
        r=next(r for r in s['responses'] if r['result'].get('raw',{}).get('error')=='synthetic_rejected');r['result']={'raw':{'ok':True}}
        with self.assertRaises(RuntimeError):self.check('first-metadata-rejected','checked',s)

    def test_settings_ack_does_not_wait_for_held_run(self):
        s=server('first-settings-rejected-held','published-while-held');self.check('first-settings-rejected-held','published-while-held',s)
        n=s['held'][0];self.assertEqual(s['requests'][n-1]['method'],F.RUN)
        self.assertTrue(any(r['method']==F.PUBLISH for r in s['requests'][n:]));self.assertNotIn(n,[r['serial'] for r in s['settlements']])

    def test_late_manager_content_mismatch_is_independent(self):
        case='first-settings-rejected-held';s=server(case,'checked',late_managers=True)
        self.check(case,'checked',s);p=attempt(case,'checked',s);self.assertFalse(p['attempts'][0]['resolved'])
        F.attempt_proof(case,'checked',p,s,F.stage_state(case,'checked'))
        p['attempts'][0]['resolved']=True
        with self.assertRaises(RuntimeError):F.attempt_proof(case,'checked',p,s,F.stage_state(case,'checked'))

    def test_unknown_matching_readback_cannot_ack(self):
        c='first-primary-unknown';s=server(c,'checked');p=attempt(c,'checked',s)
        self.assertTrue(s['current']['workflow']['is_published']);self.assertFalse(p['attempts'][0]['complete'])
        p['attempts'][0]['publication']['outcome']='acknowledged'
        with self.assertRaises(RuntimeError):F.attempt_proof(c,'checked',p,s,F.stage_state(c,'checked'))

    def test_stale_false_readback_cannot_resolve(self):
        c='first-stale-readback';s=server(c,'checked');p=attempt(c,'checked',s)
        self.assertTrue(p['attempts'][0]['complete']);p['attempts'][0]['resolved']=True
        with self.assertRaises(RuntimeError):F.attempt_proof(c,'checked',p,s,F.stage_state(c,'checked'))

    def test_cancel_zero_io(self):
        a=server('first-cancel','details');b=server('first-cancel','cancelled');self.assertEqual(a,b)
        b['current']['workflow']['is_published']=True
        with self.assertRaises(RuntimeError):self.check('first-cancel','cancelled',b)

    def test_add_remove_order_cannot_swap(self):
        s=server('first-settings','confirmed');rows=s['requests'];a=next(i for i,r in enumerate(rows) if r['method']==F.ADD);b=next(i for i,r in enumerate(rows) if r['method']==F.REMOVE)
        rows[a]['method'],rows[b]['method']=rows[b]['method'],rows[a]['method']
        with self.assertRaises(RuntimeError):self.check('first-settings','confirmed',s)

    def test_raw_opaque_loss_rejected(self):
        s=server('first-success','confirmed');row=next(r for r in s['responses'] if r['result'].get('raw',{}).get('opaque_receipt')=='first-publish');del row['result']['raw']['opaque_receipt']
        with self.assertRaises(RuntimeError):self.check('first-success','confirmed',s)

    def test_foreign_owner_refused(self):
        s=server('first-success','confirmed');s['requests'][0]['owner']['account']='foreign'
        with self.assertRaises(RuntimeError):self.check('first-success','confirmed',s)

    def test_missing_delivery_refused(self):
        s=server('first-retirement','late-settled');s['releases'][0]['delivered']=False
        with self.assertRaises(RuntimeError):self.check('first-retirement','late-settled',s)

    def test_unobserved_settlement_refused(self):
        s=server('first-retirement','primary-held');s['settlements'].append(dict(serial=s['held'][0],returned=True))
        with self.assertRaises(RuntimeError):self.check('first-retirement','primary-held',s)

    def test_picker_tokens_and_factory_drift_refused(self):
        for key,value in [('token','secret'),('count',25),('query','unrequested')]:
            s=server('first-settings','selected');r=next(r for r in s['requests'] if r['phase']=='picker');r['body'][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):self.check('first-settings','selected',s)

    def test_extra_file_crop_and_identity_refused(self):
        s=server('first-avatar','cropped');r=next(r for r in s['requests'] if r['method']=='avatars.crop');r['request']['fields'][0][1]='FOREIGN'
        with self.assertRaises(RuntimeError):self.check('first-avatar','cropped',s)

    def test_actual_upload_still_requires_png_bytes_descriptor(self):
        for key,value in [('mime',F.DOM_FILE_TYPE),('length',0),('width',0),('filename',str(F.AVATAR_PATH))]:
            s=server('first-avatar','uploaded');r=next(r for r in s['requests'] if r['method']=='avatars.upload');r['request']['image'][key]=value
            with self.subTest(key=key),self.assertRaises(RuntimeError):self.check('first-avatar','uploaded',s)

    def test_nondefault_source_constants_retained(self):
        a=F.original('first-default');b=F.original('first-settings')
        self.assertNotIn('last_published_version_id',a['workflow']);self.assertFalse(a['workflow']['is_published'])
        self.assertEqual(b['workflow']['collaborators'],['UWORKFLOW','UMANOLD01'])
        self.assertEqual(b['run']['permission_type'],'app_collaborators')

    def test_retained_operation_arguments_are_independent(self):
        c='first-settings';s=server(c,'confirmed');p=attempt(c,'confirmed',s)
        p['attempts'][0]['publication']['arguments']['workflow_id']='WfFOREIGN1'
        with self.assertRaises(RuntimeError):F.attempt_proof(c,'confirmed',p,s,F.stage_state(c,'confirmed'))


class NativeTests(unittest.TestCase):
    def test_failed_terminal_retained_without_acceptance_or_closed_claim(self):
        receipt=dict(fixture='workflow-editor',ok=False,error='Actual timeout; full retained diagnostic')
        evidence=dict(ok=False,native={})
        self.assertTrue(F.retain_native_failure(evidence,[receipt],'workflow-editor','first-avatar'))
        self.assertEqual(evidence['native'],receipt)
        self.assertFalse(evidence['ok']);self.assertNotIn('closedLogRevalidated',evidence)
        receipt['error']='changed input';self.assertNotEqual(evidence['native']['error'],receipt['error'])

    def test_foreign_or_ambiguous_failure_not_retained_as_owned_native(self):
        receipt=dict(fixture='workflow-editor',ok=False,error='Actual timeout')
        for rows in ([dict(receipt,fixture='other')],[dict(receipt,firstPublishCase='first-default')],
                [dict(receipt,scenario='managed')],[receipt,receipt],[dict(receipt,ok=True)]):
            evidence=dict(ok=False,native={})
            with self.subTest(rows=rows):self.assertFalse(F.retain_native_failure(evidence,rows,'workflow-editor','first-avatar'))
            self.assertEqual(evidence,dict(ok=False,native={}))

    def test_compact_native_failure_surfaces_cause_without_mutating_receipt(self):
        receipt=dict(fixture='workflow-editor',ok=False,error='Slack link DOM assertion timed out; avatar native=full retained diagnostic')
        before=copy.deepcopy(receipt)
        with self.assertRaisesRegex(RuntimeError,r'First-Publish native failure \(first-avatar\): Slack link DOM assertion timed out'):
            F.validate('workflow-editor','first-avatar',receipt,'.',[800,600],None,None,png=False)
        self.assertEqual(receipt,before)

    def test_failure_reporting_rejects_foreign_identity(self):
        for patch in (dict(fixture='foreign'),dict(firstPublishCase='first-default'),dict(scenario='managed')):
            receipt=dict(fixture='workflow-editor',ok=False,error='Do not surface foreign cause');receipt.update(patch)
            with self.subTest(patch=patch),self.assertRaisesRegex(RuntimeError,'First-Publish terminal identity'):
                F.validate('workflow-editor','first-avatar',receipt,'.',[800,600],None,None,png=False)

    def test_failure_reporting_does_not_accept_partial_success(self):
        receipt=dict(fixture='workflow-editor',ok=True,error='Not a success receipt')
        with self.assertRaisesRegex(RuntimeError,'First-Publish terminal identity'):
            F.validate('workflow-editor','first-avatar',receipt,'.',[800,600],None,None,png=False)

    def sample(self,case='first-success',stage='details'):
        model=dict(title=F.TITLE,description=F.DESCRIPTION,pending=False)
        def paint(id,text):
            return dict(id=id,text=text,nodeId=10,visible=True,hit=True,disabled=False,
                rect=dict(left=10,top=10,right=110,bottom=40,width=100,height=30))
        native=dict(ok=True,noOverflow=True,viewport=[800,600],composing=False,present=True,
            originalNodes=True,fields={k:dict(value=v,readonly=True,nodeId=i+1)
                for i,(k,v) in enumerate((('title',F.TITLE),('description',F.DESCRIPTION)))},
            modal=dict(originalNodes=True,fields=dict(title=dict(value=F.LOCAL_TITLE),
                description=dict(value=F.LOCAL_DESCRIPTION)),upload=dict(type='file',
                accept='image/jpeg,image/gif,image/png',files=[])),
            controls=[paint('workflow-finish-submit','Next'),paint('workflow-finish-cancel','Cancel')],
            feedback=[],outcomes=[])
        if case=='first-avatar' and stage=='uploaded':
            native['modal']['upload']['files']=[dict(name=str(F.AVATAR_PATH),type=F.DOM_FILE_TYPE,size=0)]
        sample_pane(native,stage)
        return model,native

    def check(self,model,native,case='first-success',stage='details'):
        def rect(r,viewport):
            if not (0<=r['left']<r['right']<=viewport[0] and 0<=r['top']<r['bottom']<=viewport[1]):
                raise RuntimeError('Clipped original')
        F.native_proof(case,stage,native,model,[800,600],rect)

    def test_original_parent_and_modal_buffers(self):
        model,native=self.sample();self.check(model,native)
        for target,key,value in [('parent','value','lost draft'),('parent','readonly',False),
                ('modal','value','wrong local title')]:
            bad=copy.deepcopy(native)
            row=bad['fields']['title'] if target=='parent' else bad['modal']['fields']['title']
            row[key]=value
            with self.subTest(target=target,key=key),self.assertRaises(RuntimeError):self.check(model,bad)

    def test_replaced_original_nodes_rejected(self):
        model,native=self.sample()
        for target in ('parent','modal'):
            bad=copy.deepcopy(native)
            (bad if target=='parent' else bad['modal'])['originalNodes']=False
            with self.subTest(target=target),self.assertRaises(RuntimeError):self.check(model,bad)

    def test_occluded_and_clipped_controls_rejected(self):
        model,native=self.sample()
        for key in ('visible','hit'):
            bad=copy.deepcopy(native);bad['controls'][0][key]=False
            with self.subTest(key=key),self.assertRaises(RuntimeError):self.check(model,bad)
        bad=copy.deepcopy(native);bad['controls'][0]['rect']['right']=900
        with self.assertRaises(RuntimeError):self.check(model,bad)

    def test_native_file_selection_and_unresolved_crop(self):
        model,native=self.sample('first-avatar','uploaded');self.check(model,native,'first-avatar','uploaded')
        for mutation in ('file','enabled'):
            bad=copy.deepcopy(native)
            if mutation=='file':bad['modal']['upload']['files']=[]
            else:bad['controls'].append(dict(bad['controls'][0],id='workflow-finish-submit',text='Next'))
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):self.check(model,bad,'first-avatar','uploaded')

    def test_file_name_requires_exact_repository_fixture_path(self):
        model,native=self.sample('first-avatar','uploaded')
        for path in ('avatar.png',str(F.AVATAR_PATH.parent/'other.png'),str(F.AVATAR_PATH.parent.parent/'avatar.png')):
            bad=copy.deepcopy(native);bad['modal']['upload']['files'][0]['name']=path
            with self.subTest(path=path),self.assertRaises(RuntimeError):self.check(model,bad,'first-avatar','uploaded')

    def test_zero_byte_native_shim_descriptor_is_not_multipart_png(self):
        model,native=self.sample('first-avatar','uploaded')
        for key,value in [('type','image/png'),('type',''),('type','application/octet-stream'),('size',1),('size',False),('size',None)]:
            bad=copy.deepcopy(native);bad['modal']['upload']['files'][0][key]=value
            with self.subTest(key=key,value=value),self.assertRaises(RuntimeError):self.check(model,bad,'first-avatar','uploaded')

    def test_chooser_path_matches_absolute_fixture_not_slash_spelling(self):
        self.assertTrue(F.avatar_request_path(str(F.AVATAR_PATH)))
        self.assertTrue(F.avatar_request_path(F.AVATAR_PATH.as_posix()))
        for path in ('avatar.png','src/desktop/app_home/about/management/fixture/first_publish/avatar.png',str(F.AVATAR_PATH.parent/'other.png'),None,''):
            with self.subTest(path=path):self.assertFalse(F.avatar_request_path(path))

    def avatar_confirmation(self,cropped=False):
        stage='cropped' if cropped else 'uploaded';model,native=self.sample('first-avatar','uploaded')
        local=dict(defaults=True,caption='Next',epoch=19,title=F.LOCAL_TITLE,description=F.LOCAL_DESCRIPTION,
            showMore=False,valid=cropped,iconUrl='https://example.test/first-publish-crop.png' if cropped else '',
            avatar=dict(pane=F.expected_pane(stage),status=F.expected_status(stage),paneBound=True,pending=False,cropBound=True,uploaded=None if cropped else dict(id='AVATAR_FIRST'),crop=dict(x=0,y=0,size=256)))
        confirmation=dict(id=7,visit=2,document=3,current=True,local=local,copy='collaborators',
            runDraft=dict(mode='everyone'),managerDraft=dict(ids=['UWORKFLOW'],pending=False,speedBump=False))
        product=dict(confirmation=confirmation,document=dict(id=3));model['visit']=2
        m=native['modal'];m.update(owner='7',epoch='19',icon=local['iconUrl'],moreVisible=False,crop=not cropped)
        m['custody']=dict(savedPresent=True,savedModalSame=True,savedFinishSame=True,cropSavedPresent=not cropped,
            cropSavedLength=None if cropped else 3,finishBinding=dict(epoch=18 if cropped else 19,status='ready'),
            file=dict(selectedCount=1,sameSaved=True,selectedIdentity=50,savedIdentity=50,name=str(F.AVATAR_PATH)),
            cropNodes=[dict(id='workflow-finish-crop-'+k,nodeId=None if cropped else 60+i,savedSame=not cropped,
                connected=not cropped,disabled=None if cropped else False,badInput=None if cropped else False,
                value=None if cropped else str(local['avatar']['crop'][k])) for i,k in enumerate(('x','y','size'))])
        sample_pane(native,stage)
        return stage,product,native,model

    def test_uploaded_requires_actual_bound_enabled_original_controls(self):
        stage,p,n,m=self.avatar_confirmation();F.confirmation_proof('first-avatar',stage,p,n,m)
        for which in ('bound','enabled','same','epoch','value','file'):
            a,b=copy.deepcopy(p),copy.deepcopy(n);c=b['modal']['custody']
            if which=='bound':a['confirmation']['local']['avatar']['cropBound']=False
            elif which=='enabled':c['cropNodes'][0]['disabled']=True
            elif which=='same':c['cropNodes'][0]['savedSame']=False
            elif which=='epoch':c['finishBinding']['epoch']=18
            elif which=='value':c['cropNodes'][0]['value']='64'
            else:c['file']['sameSaved']=False
            with self.subTest(which=which),self.assertRaises(RuntimeError):F.confirmation_proof('first-avatar',stage,a,b,m)

    def test_cropped_requires_removed_subtree_and_exact_icon_not_reset_ticket(self):
        stage,p,n,m=self.avatar_confirmation(True);F.confirmation_proof('first-avatar',stage,p,n,m)
        for which in ('subtree','icon','node'):
            bad=copy.deepcopy(n)
            if which=='subtree':bad['modal']['crop']=True
            elif which=='icon':bad['modal']['icon']='https://example.test/foreign.png'
            else:bad['modal']['custody']['cropNodes'][0]['nodeId']=60
            with self.subTest(which=which),self.assertRaises(RuntimeError):F.confirmation_proof('first-avatar',stage,p,bad,m)


class CropRecoveryTests(unittest.TestCase):
    def rejection(self,case='first-avatar-crop-retry'):
        _,p,n,m=NativeTests().avatar_confirmation()
        n['nodeId']=100;n['modal']['nodeId']=101;n['modal']['upload']['nodeId']=102
        for i,k in enumerate(('title','description')):n['modal']['fields'][k]['nodeId']=103+i
        p['confirmation']['revision']=5
        p['document'].update(revision=5,title=F.TITLE,description=F.DESCRIPTION,editor={},trigger={},pending=None)
        p['attempts']=[]
        a=p['confirmation']['local']['avatar'];a.update(status='error',crop=dict(x=64.0,y=64.0,size=128.0),
            uploaded=dict(id='AVATAR_FIRST',url='https://example.test/first-publish-upload.png',width=256,height=256),notice=F.CROP_NOTICE)
        for node,k in zip(n['modal']['custody']['cropNodes'],('x','y','size')):node['value']=str(a['crop'][k])
        n['feedback']=[dict(text=F.CROP_NOTICE)]
        srv=server(case,'crop-rejected')
        p['avatarReceipts']=[dict(id=7,generation=20+i,failed=False,request=r['request'],raw=srv['responses'][r['serial']-1]['result']['raw']) for i,r in enumerate(srv['requests']) if r['phase']=='avatar']
        a['receipts']=[dict(kind='upload' if i==0 else 'crop',raw=r['raw']) for i,r in enumerate(p['avatarReceipts'])]
        row=dict(stage='crop-rejected',model=m,proof=dict(firstPublish=p,server=srv,native=n))
        after=F.observation(row);before=copy.deepcopy(after);before['server']=server(case,'uploaded')
        b=before['firstPublish'];b['avatarReceipts'].pop();b['confirmation']['local']['avatar']['receipts'].pop();b['confirmation']['local']['avatar']['notice']=None
        before['native']['feedback']=[]
        row['proof']['exercise']=dict(before=before,after=copy.deepcopy(after),quiet=dict(before=copy.deepcopy(after),after=copy.deepcopy(after),observedMs=500))
        return row

    def transition(self,cancel=False):
        case='first-avatar-crop-cancel' if cancel else 'first-avatar-crop-retry';previous=self.rejection(case)
        row=copy.deepcopy(previous);row['stage']='crop-cancelled' if cancel else 'cropped';p=row['proof'];p.pop('exercise');p['server']=server(case,row['stage'])
        c=p['firstPublish']['confirmation'];a=c['local']['avatar'];a.update(uploaded=None,notice=None,pane='details',status='idle');c['local']['valid']=True
        c['local']['iconUrl']='' if cancel else 'https://example.test/first-publish-crop.png'
        n=p['native'];n['feedback']=[];n['controls'][0]['disabled']=False;n['modal'].update(icon=c['local']['iconUrl'],crop=False)
        custody=n['modal']['custody'];custody.update(cropSavedPresent=False,cropSavedLength=None)
        for node in custody['cropNodes']:node.update(nodeId=None,connected=False,savedSame=False,value=None,disabled=None,badInput=None)
        if not cancel:
            req=p['server']['requests'][-1];raw=p['server']['responses'][-1]['result']['raw']
            p['firstPublish']['avatarReceipts'].append(dict(id=7,generation=99,failed=False,request=req['request'],raw=raw));a['receipts'].append(dict(kind='crop',raw=raw))
        sample_pane(n,row['stage'])
        p['exercise']=dict(before=F.observation(previous),after=copy.deepcopy(F.observation(row)))
        return case,row,previous

    def test_original_rejection_and_both_transitions(self):
        row=self.rejection();F.recovery_proof('first-avatar-crop-retry',row,None)
        F.confirmation_proof('first-avatar-crop-retry','crop-rejected',row['proof']['firstPublish'],row['proof']['native'],row['model'])
        for cancel in (False,True):
            case,row,previous=self.transition(cancel);F.recovery_proof(case,row,previous)
            F.confirmation_proof(case,row['stage'],row['proof']['firstPublish'],row['proof']['native'],row['model'])

    def test_rejection_application_and_raw_cannot_be_promoted(self):
        for mutation in ('applied','opaque','ack','settlement'):
            s=server('first-avatar-crop-retry','crop-rejected');r=s['requests'][-1]
            if mutation=='applied':r['applied']=True
            elif mutation=='opaque':s['responses'][-1]['result']['raw']['opaque_receipt']='replaced'
            elif mutation=='ack':s['responses'][-1]['result']['raw']['ok']=True
            else:s['settlements'][-1]['returned']=False
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.server_proof('first-avatar-crop-retry','crop-rejected',s)

    def test_retry_and_cancel_exact_request_counts(self):
        for case,stage,other in [('first-avatar-crop-retry','cropped','crop-rejected'),('first-avatar-crop-cancel','crop-cancelled','uploaded')]:
            with self.subTest(case=case),self.assertRaises(RuntimeError):F.server_proof(case,stage,server(case,other))
        s=server('first-avatar-crop-cancel','cancelled');s['current']['workflow']['is_published']=True
        with self.assertRaises(RuntimeError):F.server_proof('first-avatar-crop-cancel','cancelled',s)

    def test_same_crop_identity_coordinates_and_duplicate_fields(self):
        for mutation in ('identity','coordinates','duplicate'):
            s=server('first-avatar-crop-retry','cropped');fields=s['requests'][-1]['request']['fields']
            if mutation=='identity':fields[0][1]='FOREIGN'
            elif mutation=='coordinates':fields[1][1]='0,0,256'
            else:fields.append(fields[0])
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.server_proof('first-avatar-crop-retry','cropped',s)

    def test_quiet_auto_work_and_node_replacement_refused(self):
        for mutation in ('quiet','file','crop','draft','raw'):
            row=self.rejection();p=row['proof']
            if mutation=='quiet':p['exercise']['quiet']['after']['server']['requests'].pop()
            elif mutation=='file':p['exercise']['before']['native']['modal']['custody']['file']['selectedIdentity']=999
            elif mutation=='crop':p['exercise']['before']['native']['modal']['custody']['cropNodes'][0]['nodeId']=999
            elif mutation=='draft':p['exercise']['before']['model']['title']='foreign'
            else:p['firstPublish']['avatarReceipts'][-1]['raw']={}
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.recovery_proof('first-avatar-crop-retry',row,None)

    def test_hidden_rejection_and_stale_guidance_refused(self):
        row=self.rejection();p=row['proof'];p['native']['feedback']=[]
        with self.assertRaises(RuntimeError):F.confirmation_proof('first-avatar-crop-retry','crop-rejected',p['firstPublish'],p['native'],row['model'])
        for cancel in (True,False):
            case,row,previous=self.transition(cancel);row['proof']['firstPublish']['confirmation']['local']['avatar']['notice']=F.CROP_NOTICE
            with self.subTest(cancel=cancel),self.assertRaises(RuntimeError):F.recovery_proof(case,row,previous)

    def test_cancel_cannot_apply_icon_or_replace_source(self):
        case,row,previous=self.transition(True)
        row['proof']['firstPublish']['confirmation']['local']['iconUrl']='https://example.test/first-publish-crop.png'
        row['proof']['exercise']['after']=copy.deepcopy(F.observation(row))
        with self.assertRaises(RuntimeError):F.recovery_proof(case,row,previous)


class PaneTests(unittest.TestCase):
    def row(self,case,stage,owner=7,file_id=50):
        _,p,n,m=NativeTests().avatar_confirmation(stage not in ('uploaded','crop-rejected'))
        c=p['confirmation'];c.update(id=owner,revision=5)
        p['document'].update(revision=5,title=F.TITLE,description=F.DESCRIPTION,editor={},trigger={},pending=None)
        p['attempts']=[];n['modal'].update(nodeId=100+owner,owner=str(owner));n['modal']['upload']['nodeId']=102
        a=c['local']['avatar'];a.update(pane=F.expected_pane(stage),status=F.expected_status(stage),pending=stage=='upload-held',generation=80,receipts=[],notice=F.CROP_NOTICE if stage=='upload-rejected' else None)
        c['local'].update(iconUrl='',valid=F.expected_pane(stage)=='details');n['modal']['icon']=''
        if stage=='uploaded':a['uploaded']=dict(id='AVATAR_FIRST',url='https://example.test/first-publish-upload.png',width=256,height=256)
        n['modal']['custody']['file'].update(selectedIdentity=file_id,savedIdentity=file_id)
        srv=server(case,stage);returned={r['serial'] for r in srv['settlements']}
        sent=[r for r in srv['requests'] if r['phase']=='avatar' and r['serial'] in returned]
        p['avatarReceipts']=[dict(id=7,generation=20+i,failed=False,request=r['request'],raw=srv['responses'][r['serial']-1]['result']['raw']) for i,r in enumerate(sent)]
        a['receipts']=[dict(kind='upload' if r['method']=='avatars.upload' else 'crop',raw=srv['responses'][r['serial']-1]['result']['raw']) for r in sent]
        n['feedback']=[dict(text=a['notice'])] if a['notice'] else [dict(text='Uploading your photo\u2026')] if stage=='upload-held' else []
        if stage in ('details','reopened-finish','late-settled'):
            n['modal']['upload']['files']=[];n['modal']['custody']['file'].update(selectedIdentity=None,savedIdentity=None,selectedCount=0,sameSaved=False,name=None)
        sample_pane(n,stage)
        return dict(stage=stage,model=m,proof=dict(firstPublish=p,server=srv,native=n))

    def exercise(self,case,stage):
        row=self.row(case,stage);after=F.observation(row)
        previous=self.row(case,'details') if stage in ('upload-rejected','upload-held') else self.row(case,'uploaded' if case=='first-avatar-crop-back' else 'upload-rejected')
        before=F.observation(previous)
        if stage in ('back-details','retry-details'):
            a=row['proof']['firstPublish']['confirmation']['local']['avatar'];a['generation']=81
            after=F.observation(row)
        action='workflow-finish-upload-retry' if stage=='retry-details' else 'workflow-finish-crop-back' if case=='first-avatar-crop-back' else 'workflow-finish-upload-back'
        row['proof']['exercise']=dict(before=before,after=copy.deepcopy(after),action=action)
        if stage in ('upload-rejected','upload-held'):
            row['proof']['exercise']['quiet']=dict(before=copy.deepcopy(after),after=copy.deepcopy(after),observedMs=500)
        return row,previous

    def test_active_upload_crop_and_hidden_original_details(self):
        for case,stage in [('first-avatar-upload-back','upload-rejected'),('first-avatar-upload-cancel-held','upload-held'),('first-avatar-crop-back','uploaded')]:
            row=self.row(case,stage);p=row['proof'];F.pane_native(case,stage,p['native'],False)
            F.confirmation_proof(case,stage,p['firstPublish'],p['native'],row['model'])
            F.recovery_proof(case,row,None)

    def test_hidden_controls_cannot_be_reported_active_or_replaced(self):
        for mutation in ('visible','missing','replaced','file-enabled','field-editable','pane-binding','wrong-caption'):
            row=self.row('first-avatar-upload-back','upload-rejected');n=row['proof']['native'];m=n['modal']
            if mutation=='visible':n['controls'].append(dict(n['controls'][0],id='workflow-finish-submit'))
            elif mutation=='missing':m['inactiveControls'].pop()
            elif mutation=='replaced':m['inactiveControls'][0]['nodeId']=999
            elif mutation=='file-enabled':m['upload']['disabled']=False
            elif mutation=='field-editable':m['fields']['title']['readonly']=False
            elif mutation=='pane-binding':m['activePane']['bindingEpoch']='1'
            else:n['controls'][0]['text']='Cancel'
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.pane_native('first-avatar-upload-back','upload-rejected',n,False)

    def test_original_upload_rejection_and_held_quiet(self):
        for case,stage in [('first-avatar-upload-back','upload-rejected'),('first-avatar-upload-cancel-held','upload-held')]:
            row,previous=self.exercise(case,stage);F.pane_exercise(case,row,previous)
            row['proof']['exercise']['quiet']['after']['server']['requests'].pop()
            with self.assertRaises(RuntimeError):F.pane_exercise(case,row,previous)

    def test_back_and_different_image_retire_only_original_generation(self):
        for case,stage in [('first-avatar-upload-back','back-details'),('first-avatar-upload-retry','retry-details'),('first-avatar-crop-back','back-details')]:
            row,previous=self.exercise(case,stage);F.pane_exercise(case,row,previous)
            for mutation in ('icon','file','generation','request'):
                bad=copy.deepcopy(row);a=bad['proof']['firstPublish']['confirmation']['local'];n=bad['proof']['native']['modal']
                if mutation=='icon':a['iconUrl']='forged'
                elif mutation=='file':n['custody']['file']['selectedIdentity']=555
                elif mutation=='generation':a['avatar']['generation']=80
                else:bad['proof']['server']['requests'].pop()
                bad['proof']['exercise']['after']=F.observation(bad)
                with self.subTest(case=case,mutation=mutation),self.assertRaises(RuntimeError):F.pane_exercise(case,bad,previous)

    def test_upload_rejection_raw_application_and_retry_order(self):
        for mutation in ('ack','applied','extra','wire'):
            srv=server('first-avatar-upload-retry','uploaded')
            if mutation=='ack':srv['responses'][-2]['result']['raw']['ok']=True
            elif mutation=='applied':srv['requests'][-2]['applied']=True
            elif mutation=='extra':srv['requests'].append(copy.deepcopy(srv['requests'][-1]))
            else:srv['requests'][-1]['request']['fields'][0][1]='wrong-reason'
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.server_proof('first-avatar-upload-retry','uploaded',srv)

    def test_held_upload_is_not_account_retirement_or_rollback(self):
        for stage in ('upload-held','cancelled','reopened-finish','late-settled'):
            srv=server('first-avatar-upload-cancel-held',stage);F.server_proof('first-avatar-upload-cancel-held',stage,srv)
            for mutation in ('owner','applied','settled'):
                bad=copy.deepcopy(srv)
                if mutation=='owner':bad['ownerCurrent']=False
                elif mutation=='applied':bad['requests'][-1]['applied']=False
                elif stage=='late-settled':bad['releases'][0]['delivered']=False
                else:bad['settlements'].append(dict(serial=13,returned=True))
                with self.subTest(stage=stage,mutation=mutation),self.assertRaises(RuntimeError):F.server_proof('first-avatar-upload-cancel-held',stage,bad)

    def test_chooser_inventory_and_identity_per_actual_selection(self):
        case='first-avatar-upload-retry'
        for serial in (1,2):
            r=dict(pid=123,fixture='workflow-editor',case=case,input='workflow-finish-upload',path=str(F.AVATAR_PATH),serial=serial)
            F.chooser_request(r,case,'workflow-editor',123,serial)
            for key,value in [('pid',124),('case','first-avatar'),('input','foreign'),('serial',3),('path','avatar.png')]:
                bad=dict(r);bad[key]=value
                with self.subTest(serial=serial,key=key),self.assertRaises(RuntimeError):F.chooser_request(bad,case,'workflow-editor',123,serial)
        self.assertEqual(F.chooser_count('first-avatar'),1)
        self.assertEqual(F.chooser_count(case),2)
        self.assertEqual(F.chooser_count('first-success'),0)


    def test_explicit_second_chooser_alone_replaces_file_identity(self):
        case='first-avatar-upload-retry';previous=self.row(case,'retry-details');row=self.row(case,'uploaded',file_id=51)
        row['proof']['exercise']=dict(before=F.observation(previous),after=copy.deepcopy(F.observation(row)))
        F.pane_exercise(case,row,previous)
        row['proof']['native']['modal']['custody']['file']['selectedIdentity']=50
        row['proof']['exercise']['after']=copy.deepcopy(F.observation(row))
        with self.assertRaises(RuntimeError):F.pane_exercise(case,row,previous)

    def held_tail(self):
        case='first-avatar-upload-cancel-held';held=self.row(case,'upload-held')
        cancelled=copy.deepcopy(held);cancelled['stage']='cancelled';p=cancelled['proof'];p['server']=server(case,'cancelled');p['firstPublish']['confirmation']=None;p['native']['modal']=None
        for v in p['native']['fields'].values():v['readonly']=False
        p['exercise']=dict(before=F.observation(held),after=copy.deepcopy(F.observation(cancelled)))
        successor=self.row(case,'reopened-finish',owner=8);p=successor['proof'];p['firstPublish']['confirmation']['local']['avatar']['receipts']=[]
        p['exercise']=dict(before=F.observation(cancelled),after=copy.deepcopy(F.observation(successor)),remount=dict.fromkeys(('ok','originalDetached','newRoot','newTitle','newDescription','newUpload'),True))
        late=copy.deepcopy(successor);late['stage']='late-settled';p=late['proof'];p.pop('exercise');p['server']=server(case,'late-settled');req=p['server']['requests'][-1]
        p['firstPublish']['avatarReceipts']=[dict(id=7,generation=80,failed=False,request=req['request'],raw=p['server']['responses'][-1]['result']['raw'])]
        after=F.observation(late);p['exercise']=dict(before=F.observation(successor),after=copy.deepcopy(after),release=p['server']['releases'][0],quiet=dict(before=copy.deepcopy(after),after=copy.deepcopy(after),observedMs=500))
        return case,held,cancelled,successor,late

    def test_close_reopen_and_late_original_receipt_do_not_mutate_successor(self):
        case,held,cancelled,successor,late=self.held_tail()
        for previous,row in [(held,cancelled),(cancelled,successor),(successor,late)]:
            F.pane_exercise(case,row,previous);F.recovery_proof(case,row,previous)
        F.held_upload_origin(late,held)
        for field in ('id','generation'):
            bad=copy.deepcopy(late);bad['proof']['firstPublish']['avatarReceipts'][-1][field]=999
            with self.subTest(field=field),self.assertRaises(RuntimeError):F.held_upload_origin(bad,held)
        for mutation in ('icon','native','owner','extra-read','lost-raw'):
            bad=copy.deepcopy(late);p=bad['proof']
            if mutation=='icon':p['firstPublish']['confirmation']['local']['iconUrl']='late-adopted'
            elif mutation=='native':p['native']['modal']['nodeId']=999
            elif mutation=='owner':p['firstPublish']['avatarReceipts'][-1]['id']=8
            elif mutation=='extra-read':p['server']['requests'].append(copy.deepcopy(p['server']['requests'][-1]))
            else:p['firstPublish']['avatarReceipts']=[]
            after=F.observation(bad);p['exercise']['after']=copy.deepcopy(after);p['exercise']['quiet'].update(before=copy.deepcopy(after),after=copy.deepcopy(after))
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.pane_exercise(case,bad,successor)

    def test_cancel_boundary_cannot_change_parent_metadata_or_lock(self):
        case,held,cancelled,_,_=self.held_tail()
        for mutation in ('title','node','lock','revision'):
            bad=copy.deepcopy(cancelled);p=bad['proof']
            if mutation=='title':p['native']['fields']['title']['value']='forged'
            elif mutation=='node':p['native']['fields']['title']['nodeId']=999
            elif mutation=='lock':p['native']['fields']['title']['readonly']=True
            else:p['firstPublish']['document']['revision']=99
            p['exercise']['after']=copy.deepcopy(F.observation(bad))
            with self.subTest(mutation=mutation),self.assertRaises(RuntimeError):F.pane_exercise(case,bad,held)


if __name__=='__main__':unittest.main()
