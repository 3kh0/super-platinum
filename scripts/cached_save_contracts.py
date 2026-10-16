"""Exact cached-mode schema; legacy original workflow remains unchanged."""
from pathlib import Path
from save_workflow_contracts import (require,identical,read_json,digest,save,unlinked,owner)
from toast_geometry import validate_toast
ALIAS='cached-viewer-save-check'
TITLE='Save file'
OLD='Cached original A\nSafe overwrite fixture.\n'
NEW='Cached original B\nSafe overwrite fixture.\n'
STAGES=('initial','canceled','saved','held','refreshed','retired')
ACTIONS=('cancel','set-overwrite','save-overwrite-1','decline','save-overwrite-2','accept','set-retire','save-retire','accept-retire')
CAPTURES=('initial','cancel-dialog','canceled','overwrite-dialog','overwrite-decline','after-decline','overwrite-accept','saved','retire-dialog','retire-accept','held','refreshed','retired')
def identity(expected):return dict(version=1,nonce=expected['nonce'],pid=expected['pid'],fixture=ALIAS)
def role(action):return 'save' if action.startswith('save-') else 'accept' if action=='accept-retire' else action
def path_index(action):return 1 if action.endswith('retire') else 0

class DestinationAudit:
    def __init__(self,directory,nonce):
        self.directory=directory;self.nonce=nonce;self.downloads=directory/'downloads'
        require(directory.name=='cached-viewer-save-'+nonce,'Exact cached root')
        self.paths=[self.downloads/(p+'-'+nonce+'.txt') for p in ('overwrite','retire')]
        self.parents=[(p,unlinked(p).st_dev,unlinked(p).st_ino) for p in (directory,self.downloads)]
        self.before=None
    def check_parents(self):
        for p,dev,ino in self.parents:
            for a in (p,*p.parents):unlinked(a)
            info=p.stat();require(info.st_dev==dev and info.st_ino==ino,'Cached directory identity')
    def file(self,path):
        info=unlinked(path);require(path.is_file(),'Cached destination regular file')
        return dict(name=path.name,identity=[info.st_dev,info.st_ino],bytes=info.st_size,sha256=digest(path))
    def create_sentinel(self):
        self.check_parents();require(not list(self.downloads.iterdir()),'Fresh empty cached downloads')
        for path,body in zip(self.paths,(b'cached-overwrite-sentinel\n',b'cached-retire-sentinel\n')):
            with path.open('xb') as out:out.write(body)
        self.before=[self.file(p) for p in self.paths];return self.before
    def snapshot(self,phase,*,saved=False,**unused):
        self.check_parents();require(self.before is not None,'Cached sentinel not initialized')
        require(set(self.downloads.iterdir())==set(self.paths),'Unexpected private file or surviving part')
        rows=[self.file(p) for p in self.paths]
        require(identical(rows[1],self.before[1]),'Retirement sentinel changed')
        if saved:
            import hashlib
            require(rows[0]['bytes']==len(OLD.encode()) and rows[0]['sha256']==hashlib.sha256(OLD.encode()).hexdigest(),'Exact cached original overwrite')
        else:require(identical(rows[0],self.before[0]),'Overwrite before authorized cached transfer')
        return dict(phase=phase,files=rows)

def validate_stage(value,index,expected,width=None):
    stage=STAGES[index]
    require(set(value)=={'ok','identity','stage','dom','ledger','observed'} and value['ok'] is True and value['stage']==stage and identical(value['identity'],identity(expected)),'Cached stage identity/schema')
    issued,returned,outcomes,held,released,reads=((0,0,0,False,False,0),(1,1,0,False,False,0),(2,2,1,False,False,0),(3,3,1,True,False,0),(3,3,1,True,False,1),(3,3,2,True,True,1))[index]
    ledger=dict(issued=issued,returns=[[1,'none'],[2,'existing'],[3,'existing']][:returned],outcomes=[[2,'saved'],[3,'retired-before-transfer']][:outcomes],transfers=[2] if issued>=2 else [],held=held,released=released,metadata=reads,bodies=reads,active=3 if stage in ('held','refreshed') else None,failed=False)
    require(identical(value['ledger'],ledger),'Exact actual cached transition ledger')
    fresh=index>=4;observed=value['observed'];dom=value['dom']
    require(set(observed)=={'old','current','fresh','oldCanceled','bytes','syntheticSession','structurallyDisconnected','toast'},'Observed cached fields')
    require(observed['fresh'] is fresh and observed['oldCanceled'] is fresh and observed['syntheticSession'] is True and observed['structurallyDisconnected'] is True and observed['bytes']==(NEW if fresh else OLD),'Cached owner/bytes/isolation')
    import re
    require(all(type(observed[k]) is str and re.fullmatch(r'super-platinum-media://attachment/[0-9a-f]{32}',observed[k]) for k in ('old','current')) and (observed['old']!=observed['current']) is fresh,'Actual asset replacement')
    require(set(dom)=={'ok','stage','viewport','button','text','toast','replaced'} and dom['ok'] is True and dom['stage']==stage and identical(dom['viewport'],[1280,780]) and dom['text']==observed['bytes'] and dom['replaced'] is fresh,'Cached DOM')
    button=dom['button'];require(set(button)=={'label','disabled','hit','rect'} and button['label']==('Cancel' if stage=='held' else 'Download') and button['disabled'] is False and button['hit'] is True,'Actual Download control')
    r=button['rect'];require(set(r)=={'left','right','top','bottom','width','height'},'Cached rectangle schema')
    import math
    require(all(type(v) in (int,float) and math.isfinite(v) for v in r.values()),'Finite cached geometry')
    require(0<=r['left']<r['right']<=1280 and 0<=r['top']<r['bottom']<=780 and r['width']>=96 and r['height']>0,'Visible actual Download rectangle')
    wanted='Saved release-notes.txt' if stage=='saved' else None
    require(observed['toast']==wanted,'Native cached toast')
    if wanted:validate_toast(dom['toast'],dom['viewport'],wanted)
    else:require(dom['toast'] is None,'Unexpected toast claim')
    return r['width']

def validate_terminal(value,stages,expected):
    require(set(value)=={'ok','identity','stages','ledger','actualRfd','programmaticActivation','syntheticSession','structurallyDisconnected','lateCommitExercised'},'Terminal cached schema')
    require(all(value[k] is True for k in ('ok','actualRfd','programmaticActivation','syntheticSession','structurallyDisconnected')) and value['lateCommitExercised'] is False,'Cached scope')
    require(identical(value['identity'],identity(expected)) and len(stages)==6 and identical(value['stages'],stages) and identical(value['ledger'],stages[-1]['ledger']),'Closed cached history')
    for i,stage in enumerate(stages):validate_stage(stage,i,expected)
    require(all(s['observed']['old']==stages[0]['observed']['old'] for s in stages),'Captured old asset changed')

def validate_logs(data,expected,stages,directory):
    require(set(data)=={'waiting','requested','returned','transfer','held','outcome','stage','check'},'Closed cached log kinds')
    ident=identity(expected)
    require(identical(data['waiting'],[ident]) and identical(data['stage'],stages),'Closed waiting/stages')
    require(identical(data['requested'],[dict(identity=ident,attempt=i,actualRfd=True,cached=True,mode='ConfirmedCachedOverwrite') for i in (1,2,3)]),'Actual rfd admissions')
    paths=[None,str(directory/'downloads'/('overwrite-'+expected['nonce']+'.txt')),str(directory/'downloads'/('retire-'+expected['nonce']+'.txt'))]
    require(identical(data['returned'],[dict(identity=ident,attempt=i,kind='none' if i==1 else 'existing',actualRfd=True,path=paths[i-1],accepted=True) for i in (1,2,3)]),'Actual returned path custody')
    require(identical(data['transfer'],[dict(identity=ident,attempt=2,cached=True)]),'Exactly one cached transfer')
    require(identical(data['held'],[dict(identity=ident,attempt=3,actualReturnedPath=True,beforeCurrentCheck=True)]),'Exact actual return checkpoint')
    require(identical(data['outcome'],[dict(identity=ident,attempt=2,kind='saved',ok=True),dict(identity=ident,attempt=3,kind='retired-before-transfer',ok=True,canceled=True)]),'Actual terminal save outcomes')
    require(len(data['check'])==1,'One closed terminal')
    validate_terminal(data['check'][0],stages,expected)


def validate_probe(probe, expected, kind, hwnd, save_hwnd=None):
    require(isinstance(probe, dict) and probe.get('ok') is True and probe.get('mode') == 'probe', 'probe outcome')
    require(all(identical(probe.get(k), v) for k, v in expected.items()), 'probe owner')
    window = probe.get('window', {})
    require(probe.get('kind') == kind and type(window.get('hwnd')) is int and window['hwnd'] == hwnd, 'probe kind/HWND')
    if kind == 'save':
        require(window.get('title') == TITLE, 'Save title')
    else:
        chain = window.get('ownerChain')
        require(isinstance(chain, list) and chain and all(type(n) is int and n > 0 for n in chain), 'measured owner chain')
        require(hwnd not in chain and save_hwnd in chain and len(chain) <= 8 and len(set(chain)) == len(chain), 'overwrite rooted at observed Save')
    require(isinstance(probe.get('controls'), list) and 0 < len(probe['controls']) <= 80, 'bounded controls')
    require(len({tuple(c['runtimeId']) for c in probe['controls']}) == len(probe['controls']), 'unique control runtime IDs')


def validate_review(review, observed):
    require(set(review) == {'version', 'stage', 'observedSha256', 'imageSha256', 'request'}, 'exact review fields')
    require(identical(review['version'], 1) and review['stage'] == observed['stage'], 'stage-bound review')
    require(review['observedSha256'] == observed['_sha256'] and review['imageSha256'] == observed['imageSha256'], 'review evidence hashes')
    probe = observed['probe']; request = review['request']; action = observed['stage']
    require(action in ACTIONS,'Only nine exact cached review actions')
    require(probe['kind']==('overwrite' if action in ('decline','accept','accept-retire') else 'save'),'Cached action/dialog relationship')
    keys = {'nonce', 'pid', 'creation', 'mode', 'kind', 'hwnd', 'window', 'target'}
    if action.startswith('set-') or action.startswith('save-'):
        keys.add('allowedPath')
    if not action.startswith('set-'):
        keys.add('role')
    if action.startswith('save-'):
        keys.add('filenameTarget')
    if probe['kind'] == 'overwrite':
        keys.add('saveHwnd')
    require(set(request) == keys, 'exact action request fields')
    require(all(identical(request[k], probe[k]) for k in ('nonce', 'pid', 'creation', 'kind')) and identical(request['window'], probe['window']) and identical(request['hwnd'], probe['window']['hwnd']), 'review owner/window')
    require(request['mode'] == ('set-path' if action.startswith('set-') else 'invoke'), 'review mode')
    target = request['target']
    require(sum(identical(target, c) for c in probe['controls']) == 1, 'exact observed target')
    require(target.get('enabled') is True and target.get('offscreen') is False, 'visible enabled target')
    if action.startswith('set-'):
        require(target.get('controlType') == 'ControlType.Edit' and target.get('value') is True and target.get('readOnly') is False, 'writable observed filename')
    else:
        require(target.get('controlType') == 'ControlType.Button' and target.get('invoke') is True, 'invokable observed button')
    if 'allowedPath' in keys:
        require(request['allowedPath'] == observed['allowedPaths'][path_index(action)], 'exact destination for stage')
    if 'role' in keys:
        require(request['role'] == role(action), 'exact reviewed role')
        name = {'cancel': 'Cancel', 'save': 'Save', 'decline': 'No', 'accept': 'Yes'}[request['role']]
        require(target.get('name') in (name, '&' + name), 'reviewed button semantic name/locale')
    if 'filenameTarget' in keys:
        filename = request['filenameTarget']
        require(sum(identical(filename, c) for c in probe['controls']) == 1, 'exact observed filename control')
        require(filename.get('controlType') == 'ControlType.Edit' and filename.get('value') is True and filename.get('readOnly') is False, 'filename pattern')
    if probe['kind'] == 'overwrite':
        require(request['saveHwnd'] == observed['saveHwnd'], 'reviewed overwrite relationship')
    return request
