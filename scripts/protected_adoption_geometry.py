"""Strict owned geometry; plain reads never replace a native admission ticket."""
import runpy
import copy


def install(h):
    globals().update(h)
    global G
    G=runpy.run_path(str(SCRIPTS/'protected_enrollment_geometry.py'))
    G['install'](h)


TEXT={'open':'Review draft','submit':'Enable recovery','cancel':'Cancel waiting','check':'Check status',
      'reload':'Reload saved drafts','adopt':'Continue editing','retry':'Retry saving','resume':'Resume editing'}


def button(v,viewport,**kwargs):
    keys(v,('id','text','label','rect','disabled','hit','textGeometry','visible'),'complete button proof')
    G['button']({k:x for k,x in v.items() if k!='textGeometry'},viewport,**kwargs)
    t=v['textGeometry'];keys(t,('rects','clientWidth','clientHeight','scrollWidth','scrollHeight'),'button label geometry')
    require(all(number(t[k]) and t[k]>=0 for k in ('clientWidth','clientHeight','scrollWidth','scrollHeight')),'Invalid label extents')
    require(t['clientWidth']>0 and t['clientHeight']>0,'Button has no content box')
    require(t['scrollWidth']<=t['clientWidth']+1 and t['scrollHeight']<=t['clientHeight']+1,'Button label is clipped')
    require(isinstance(t['rects'],list) and len(t['rects'])<=32,'Unbounded label fragments')
    require(bool(t['rects'])==bool(v['text']),'Missing actual button text range')
    for r in t['rects']:
        require(rectangle(r,viewport),'Label escaped viewport')
        b=v['rect'];require(r['x']>=b['x']-1 and r['y']>=b['y']-1 and r['x']+r['width']<=b['x']+b['width']+1 and r['y']+r['height']<=b['y']+b['height']+1,'Button label extends beyond its owned control')


def click(v,control,fixture,generation=None):
    keys(v,('ok','control','generation','clicked','proof','bodyScroll','focused'),'actual click')
    for k,x in dict(ok=True,control=control,generation=generation,clicked=True,focused=True).items():same(v[k],x,'Click '+k)
    prefix='protected-enrollment-' if control=='submit' or control=='cancel' and generation else 'protected-adoption-'
    same(v['proof']['id'],prefix+control,'Wrong actual clicked control')
    button(v['proof'],expected_viewport(fixture),text=TEXT[control])
    same(v['bodyScroll'],0 if generation else None,'Body scroll not restored')


def observation(v,fixture,target,text,locked=False,*,expected_composing=False):
    keys(v,('ok','target','before','native','readOnly','disabled','admission','fieldFocused','activeId','visibilityEvidence'),'native observation')
    for k,x in dict(ok=True,target=target,readOnly=locked,disabled=False,visibilityEvidence='not-measured').items():same(v[k],x,'Observation '+k)
    require(type(v['fieldFocused']) is bool and isinstance(v['activeId'],str),'Invalid native focus')
    if locked:require(isinstance(v['admission'],str) and v['admission'].isdigit() and int(v['admission'])>0,'Missing actual native lease')
    else:same(v['admission'],None,'Native lease not released')
    keys(v['before'],('text','start','end','direction'),'prior native value')
    native(v['native'],text,expected_viewport(fixture),expected_composing=expected_composing)


def status(v,fixture,controls,message):
    viewport=expected_viewport(fixture)
    keys(v,('ok','page','generation','surfaces','hits','buttons','scroll','viewport','documentWidth','text','title','rowKinds','pending'),'status geometry')
    for k,x in dict(ok=True,page='status',generation=None,viewport=viewport,title=None,rowKinds=[],pending=False).items():same(v[k],x,'Status '+k)
    require(number(v['documentWidth']) and 0<v['documentWidth']<=viewport[0]+1,'Document horizontal overflow')
    keys(v['surfaces'],('panel','selected'),'status bounds');keys(v['hits'],('panel','selected'),'status hits')
    for name in v['surfaces']:
        require(rectangle(v['surfaces'][name],viewport),'Clipped status '+name)
        BASE['hit_grid'](v['hits'][name],v['surfaces'][name])
    same(v['surfaces']['selected'],v['surfaces']['panel'],'Status selected surface mismatch')
    same(v['scroll'],dict(before=dict(document=0,body=None),after=dict(document=0,body=None),bodyHeight=None,bodyScrollHeight=None),'Status ancestor scroll')
    keys(v['buttons'],['protected-adoption-'+k for k in controls],'Actual status controls')
    for k,disabled in controls.items():button(v['buttons']['protected-adoption-'+k],viewport,text=TEXT[k],disabled=disabled)
    same(v['text'],message+''.join(TEXT[k] for k in controls),'Status message/control copy')


def record(v,fixture,scenario,target,generation,text,controls,message):
    stage=v['stage'];locked=stage=='expiry-locked'
    composing=scenario=='native-refusals-channel' and stage=='composition-active'
    observation(v['native'],fixture,target,text,locked,expected_composing=composing)
    if stage=='dialog':
        require(len(v['pages'])==2,'Missing complete inventory pages')
        for page,g in zip(('channel','thread'),v['pages']):
            plain=copy.deepcopy(g)
            for key,b in g['buttons'].items():
                button(b,expected_viewport(fixture),text=b['text'],disabled=b['disabled'])
                plain['buttons'][key].pop('textGeometry')
            G['surface'](plain,fixture,scenario,stage,page,target,generation,'')
        same(v['composer'],None,'Modal cannot claim unobscured composer')
    else:
        require(len(v['pages'])==1,'Unexpected viewport captures')
        status(v['pages'][0],fixture,controls,message)
        focused=v['composer'].get('fieldFocused');require(type(focused) is bool,'Missing field focus')
        BASE['geometry'](v['composer'],fixture,target,text,locked=locked,focused=focused,proof=False,expected_composing=composing)
        BASE['same_native'](v['native']['native'],v['composer']['native'])
