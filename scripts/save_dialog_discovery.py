"""Pure strict discovery/review contracts. This module never launches UI."""
from pathlib import Path
import json,re
import time
ALIAS='original-download-save-dialog'
TITLE='Save original file - choose a new filename'
def require(value,message):
    if not value:raise RuntimeError(message)
def owner(nonce,pid,creation):
    require(re.fullmatch('[0-9a-f]{32}',nonce) is not None,'nonce')
    require(type(pid) is int and pid>0 and type(creation) is int and creation>0,'owner identity')
    return dict(nonce=nonce,pid=pid,creation=creation)
def validate_probe(value,expected):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('mode')=='probe','probe outcome')
    require(all(value.get(k)==v and type(value.get(k)) is type(v) for k,v in expected.items()),'probe owner')
    require(type(value.get('hwnd')) is int and value['hwnd']>0 and value.get('title')==TITLE,'dialog identity')
    r=value.get('rect');require(isinstance(r,list) and len(r)==4 and all(type(x) is int for x in r) and r[2]>0 and r[3]>0,'dialog rectangle')
    require(isinstance(value.get('controls'),list) and 0<len(value['controls'])<=80,'bounded controls')
    for c in value['controls']:
        require(isinstance(c,dict) and isinstance(c.get('runtimeId'),list) and c['runtimeId'] and all(type(x) is int for x in c['runtimeId']),'control runtime ID')
def validate_review(review,probe,expected):
    require(set(review)=={'version','action','nonce','pid','creation','hwnd','target'},'exact review fields')
    require(type(review['version']) is int and review['version']==1 and review['action']=='cancel','review action')
    require(all(review.get(k)==v and type(review.get(k)) is type(v) for k,v in expected.items()) and type(review['hwnd']) is int and review['hwnd']==probe['hwnd'],'review owner')
    target=review['target'];require(probe['controls'].count(target)==1,'exact observed control')
    require(target.get('name')=='Cancel' and target.get('controlType')=='ControlType.Button' and target.get('enabled') is True and target.get('offscreen') is False and target.get('invoke') is True,'only observed Cancel')
    return target
def read_json(path):
    p=Path(path);s=p.lstat()
    require(p.is_file() and not p.is_symlink() and not (getattr(s,'st_file_attributes',0)&0x400) and s.st_size<=65536,'bounded regular JSON')
    def unique(rows):
        result={}
        for k,v in rows:
            require(k not in result,'duplicate JSON key');result[k]=v
        return result
    return json.loads(p.read_text(encoding='utf-8'),object_pairs_hook=unique)

def settle_samples(sample, *, clock=time.monotonic, sleep=time.sleep):
    """Bounded passive samples; None resets stability, exceptions fail immediately.

    Require at least 0.8 s after entry and three identical eligible samples
    spanning 0.3 s. This is pixel/geometry stability, not semantic paint proof.
    Every eligible sample must independently pass ownership before and after
    its physical capture. The caller retains every observation and image.
    """
    start=clock();stable_since=None;prior=None;count=0
    for index in range(40):
        if clock()-start>=6:break
        result=sample(index)
        now=clock()
        if now-start>=6:break
        if result is None:
            stable_since=None;prior=None;count=0
        else:
            key=result['fingerprint']
            if key!=prior:
                prior=key;stable_since=now;count=1
            else:count+=1
            if now-start>=.8 and count>=3 and now-stable_since>=.3:
                return result,dict(samples=index+1,elapsedSeconds=now-start,stableSeconds=now-stable_since,consecutive=count)
        sleep(.1)
    raise RuntimeError('physical capture did not settle within 6 seconds / 40 passive samples')
