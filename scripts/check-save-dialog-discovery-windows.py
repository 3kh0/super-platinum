#!/usr/bin/env python3
"""Real rfd cancellation diagnostic. No Save/overwrite action or transfer is allowed.

Writes dialog-observed.json/dialog.png, then waits for root to inspect them and
publish cancel-review.json naming one exact observed Cancel control. Never builds.
"""
import argparse,hashlib,json,os,runpy,secrets,subprocess,sys,time
from pathlib import Path
from native_fixture import ROOT,NativeFixture,DEFAULT_BINARY
from original_download_native import LocalOriginal
from save_dialog_discovery import ALIAS,owner,require,read_json,validate_probe,validate_review,settle_samples
from windows_process_metrics import WindowsAPI
from windows_capture import capture_rect
from windows_client_occlusion import sample_client_ownership
from visual_pointer import WindowsPointerIO
PREFIX='save-dialog-check: '
def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(path,value):
    with Path(path).open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n')
def records(log,prefix):
    if not Path(log).exists():return []
    require(Path(log).stat().st_size<=4*1024*1024,'bounded native log')
    return [json.loads(line.split(prefix,1)[1]) for line in Path(log).read_text(encoding='utf-8').splitlines(keepends=True) if prefix in line and line.endswith('\n')]
def one(native,prefix,seconds,verify):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        verify()
        rows=records(native.log,prefix)
        if rows:
            require(len(rows)==1,'duplicate native record');return rows[0]
        terminal=records(native.log,PREFIX)
        require(not terminal,'native failed before expected stage')
        time.sleep(.05)
    raise RuntimeError('native stage expired: '+prefix)
def ack(directory,expected,stage):
    path=directory/f'ack-{stage}.json';temp=directory/f'ack-{stage}.staged'
    save(temp,dict(version=1,nonce=expected['nonce'],pid=expected['pid'],fixture=ALIAS,stage=stage))
    try:os.link(temp,path)
    finally:temp.unlink()
def helper(directory,request,kind):
    path=directory/f'uia-{kind}.log'
    env=os.environ.copy();env['SP_SAVE_DIALOG_JSON']=json.dumps(request,separators=(',',':'))
    script=Path(__file__).with_name('windows_save_dialog_discovery.ps1')
    command=[str(Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'),'-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',str(script)]
    with path.open('x',encoding='utf-8') as output:
        process=subprocess.Popen(command,cwd=ROOT,env=env,stdout=output,stderr=output,creationflags=subprocess.CREATE_NO_WINDOW)
        lifecycle={'pid':process.pid,'kind':kind,'mode':request['mode'],'timedOut':False}
        try:
            try:code=process.wait(timeout=12)
            except subprocess.TimeoutExpired:
                lifecycle['timedOut']=True
                process.terminate();process.wait(timeout=5);raise RuntimeError('UIA helper expired; action never replayed')
        finally:
            if process.poll() is None:
                process.kill();process.wait(timeout=5)
            lifecycle.update(exitCode=process.poll(),exited=process.poll() is not None)
            save(directory/f'uia-{kind}-lifecycle.json',lifecycle)
    require(code==0,'UIA helper failed: '+path.name)
    rows=[json.loads(line) for line in path.read_text(encoding='utf-8-sig').splitlines() if line.startswith('{')]
    return rows
def empty_downloads(directory):
    require(list((directory/'downloads').iterdir())==[],'diagnostic wrote a destination')
def run(binary,review_seconds):
    require(sys.platform=='win32','Windows only')
    require(15<=review_seconds<=150,'bounded review window')
    nonce=secrets.token_hex(16);directory=(ROOT/'tmp'/f'original-save-dialog-{nonce}').resolve()
    require(directory.parent==(ROOT/'tmp').resolve(),'private tmp parent')
    directory.mkdir(exist_ok=False);(directory/'downloads').mkdir()
    for p in [directory/'downloads',*directory.parents,directory]:
        s=p.lstat();require(not p.is_symlink() and not (getattr(s,'st_file_attributes',0)&0x400),'unlinked private path')
    evidence={'ok':False,'diagnosticOnly':True,'directory':str(directory),'binary':str(binary),'binarySha256':digest(binary),'nonce':nonce,
              'limitations':['Actual rfd cancellation only; no accepted destination or overwrite coverage.','UIA invocation is synthetic native automation, not physical keyboard input.','Sampled ownership/geometry; no continuous occlusion proof.'],
              'pointerMoveAttempted':False,'captures':[],'observations':[],'captureSamples':[]}
    api=WindowsAPI();handle=None;native=None;server=None;expected=None;primary=None
    pointer=WindowsPointerIO();pointer_start=pointer.position();evidence['pointerOriginal']=pointer_start
    visual=runpy.run_path(str(ROOT/'scripts/check-visual-native.py'))
    try:
        server=LocalOriginal(nonce)
        env={'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_CHECK':'1','SUPER_PLATINUM_SAVE_DIALOG_CHECK':'1','SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR':str(directory),
             'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE':nonce,'SUPER_PLATINUM_ORIGINAL_DOWNLOAD_PORT':str(server.port)}
        native=NativeFixture(ALIAS,directory/'native.log',binary,env)
        with native:
            handle=api.open(native.process.pid);expected=owner(nonce,native.process.pid,api.times(handle)['creation_filetime']);evidence['owner']=expected
            def verify():
                native.require_running();v=api.times(handle)
                require(v['creation_filetime']==expected['creation'] and v['exit_filetime']==0,'owned process changed')
                require(pointer.position()==pointer_start,'pointer moved during diagnostic')
            def observe(hwnd=None,*,stage='check',settling=False):
                verify()  # Process/pointer changes are never treated as a paint transition.
                require(len(evidence['observations'])<512,'observation budget exhausted')
                observation={'stage':stage,'expectedHwnd':hwnd,'monotonic':time.monotonic(),'ok':False}
                evidence['observations'].append(observation)
                try:
                    v=visual['window_observation'](expected['pid']);observation['window']=v
                    # Keep the full bounded hit result even when a guard fails.
                    hit=sample_client_ownership(v);observation['hit']=hit
                    if hwnd is not None:require(v['hwnd']==hwnd,'foreground HWND changed')
                    require(v['window_within_virtual_bounds'] is True,'window outside desktop')
                    require(hit['ok'],'owned window covered at sampled points')
                except (RuntimeError,OSError) as error:
                    observation['error']=f'{type(error).__name__}: {error}'
                    if settling:return None
                    raise
                observation['ok']=True
                return v
            def capture(name,hwnd):
                samples=directory/'capture-samples';samples.mkdir(exist_ok=True)
                def sample(index):
                    row={'stage':name,'index':index,'eligible':False}
                    evidence['captureSamples'].append(row)
                    before=observe(hwnd,stage=f'{name}-{index}-before',settling=True)
                    if before is None:return None
                    r=before['window_rect_physical'];path=samples/f'{name}-{index:02}.png'
                    require(not path.exists(),'fresh capture sample')
                    capture_rect(path,*r);row.update(path=str(path),sha256=digest(path),rect=r,hwnd=hwnd)
                    after=observe(hwnd,stage=f'{name}-{index}-after',settling=True)
                    if after is None:return None
                    if any(before[k]!=after[k] for k in ('window_rect_physical','client_rect_physical')):
                        row['error']='capture geometry changed';return None
                    row['eligible']=True
                    return dict(fingerprint=(hwnd,tuple(r),tuple(after['client_rect_physical']),row['sha256']),record=row,window=after)
                result,settled=settle_samples(sample)
                source=result['record'];path=directory/f'{name}.png'
                require(not path.exists(),'fresh capture')
                # Publish the already checked physical sample without another capture.
                with path.open('xb') as output:output.write(Path(source['path']).read_bytes())
                evidence['captures'].append({'stage':name,'path':str(path),'hwnd':hwnd,'rect':source['rect'],'sha256':digest(path),'sample':source['path'],'settle':settled})
                return result['window']
            waiting=one(native,'save-dialog-waiting: ',30,verify)
            require(waiting==dict(version=1,nonce=nonce,pid=expected['pid'],fixture=ALIAS),'exact native waiting')
            native.focus();initial_window=observe();evidence['initialWindow']=initial_window
            ack(directory,expected,'ready')
            initial=one(native,'save-dialog-initial: ',15,verify)
            require(initial.get('ok') is True and initial.get('stage')=='initial','initial native DOM')
            capture('initial',initial_window['hwnd']);ack(directory,expected,'initial')
            requested=one(native,'save-dialog-requested: ',15,verify)
            require(requested==dict(version=1,nonce=nonce,pid=expected['pid'],attempt=1,actualRfd=True),'actual dialog request')
            deadline=time.monotonic()+15
            while True:
                window=observe(stage='await-dialog',settling=True)
                if window and window['hwnd']!=initial_window['hwnd']:break
                require(time.monotonic()<deadline,'native dialog did not appear');time.sleep(.1)
            window=capture('dialog',window['hwnd'])
            rows=helper(directory,dict(expected,mode='probe'),'probe');require(len(rows)==1,'one discovery inventory')
            probe=rows[0];validate_probe(probe,expected);require(probe['hwnd']==window['hwnd'] and probe['rect']==window['window_rect_physical'],'discovered HWND/geometry mismatch')
            save(directory/'dialog-observed.json',probe);observe(probe['hwnd'])
            evidence['dialog']=probe
            print(json.dumps({'stage':'awaiting-reviewed-cancel','directory':str(directory),'reviewFile':str(directory/'cancel-review.json')}),flush=True)
            deadline=time.monotonic()+review_seconds
            while not (directory/'cancel-review.json').exists():
                verify();require(not records(native.log,'save-dialog-returned: '),'dialog returned before reviewed Cancel')
                require(time.monotonic()<deadline,'Cancel review expired; no automatic action');time.sleep(.1)
            review=read_json(directory/'cancel-review.json');target=validate_review(review,probe,expected)
            observe(probe['hwnd'])
            actions=helper(directory,dict(expected,mode='cancel',hwnd=probe['hwnd'],target=target),'cancel')
            require(len(actions)==2 and actions[0].get('kind')=='before-invoke' and actions[1].get('kind')=='invoke-returned','exact Cancel invocation records')
            for action in actions:
                require(action.get('nonce')==nonce and action.get('pid')==expected['pid'] and action.get('hwnd')==probe['hwnd'] and action.get('action')=='cancel' and action.get('count')==1,'Cancel action owner')
            require(actions[0].get('target')==target,'Cancel target changed');evidence['cancelActions']=actions
            returned=one(native,'save-dialog-returned: ',15,verify)
            require(returned==dict(version=1,nonce=nonce,pid=expected['pid'],attempt=1,none=True,first=True,transferAllowed=False),'real rfd did not return None exactly once')
            terminal=one(native,PREFIX,15,verify)
            require(terminal.get('ok') is True and terminal.get('owner')==waiting and terminal.get('attempts')==1 and terminal.get('transfers')==0 and all(terminal.get(k) is True for k in ('noSession','noTransport','noAccount','actualRfd','diagnosticOnly','dialogReturnedNone')),'native cancellation terminal')
            evidence['terminal']=terminal;empty_downloads(directory)
            require(server.rows==[],'unexpected original request')
            native.focus();capture('canceled',initial_window['hwnd'])
        empty_downloads(directory)
        require(records(native.log,PREFIX)==[terminal] and records(native.log,'save-dialog-requested: ')==[requested] and records(native.log,'save-dialog-returned: ')==[returned],'closed log changed')
        evidence['closedLogValidated']=True
    except BaseException as error:
        primary=error;evidence['error']=f'{type(error).__name__}: {error}'
    finally:
        failures=[]
        def audit(name,fn):
            try:fn()
            except BaseException as e:failures.append({'stage':name,'error':f'{type(e).__name__}: {e}'})
        if server is not None:
            audit('server-close',server.close);evidence['requests']=list(server.rows)
            audit('zero-requests',lambda:require(server.rows==[],'unexpected HTTP request'))
        if handle is not None:
            def exited():
                v=api.times(handle);evidence['exitIdentity']=v
                require(v['creation_filetime']==expected['creation'] and v['exit_filetime']>=expected['creation'],'owned exit missing')
            audit('exit',exited);audit('handle-close',lambda:require(api.kernel.CloseHandle(handle),'handle close failed'))
        evidence['cleanupErrors']=[] if native is None else native.cleanup_errors
        evidence['processExited']=native is not None and native.process is not None and native.process.poll() is not None
        evidence['exitCode']=None if native is None or native.process is None else native.process.poll()
        audit('process-exit',lambda:require(evidence['processExited'],'native process exit missing'))
        audit('cleanup',lambda:require(not evidence['cleanupErrors'],'native cleanup error'))
        audit('closed-downloads',lambda:empty_downloads(directory))
        def binary_audit():
            evidence['binarySha256After']=digest(binary)
            require(evidence['binarySha256After']==evidence['binarySha256'],'binary changed')
        def pointer_audit():
            evidence['pointerFinal']=pointer.position()
            require(tuple(evidence['pointerFinal'])==tuple(pointer_start),'pointer moved')
        audit('binary',binary_audit);audit('pointer',pointer_audit)
        evidence['auditErrors']=failures;evidence['ok']=primary is None and not failures
        save(directory/'receipt.json',evidence)
        print(json.dumps({'receipt':str(directory/'receipt.json'),'ok':evidence['ok']}),flush=True)
    if primary is not None:raise primary
    require(evidence['ok'],'final diagnostic audit failed')
    return evidence
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY);parser.add_argument('--review-seconds',type=int,default=120)
    args=parser.parse_args();run(args.binary.resolve(),args.review_seconds)
