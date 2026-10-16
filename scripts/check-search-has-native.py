"""Finite disconnected Search checks. Invoke Python with -X utf8 -B; no build."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import sys
import tempfile
import time
from canvas_native import CanvasCheck,capture_stage,validate_closed
from canvas_evidence import audited_fixture,closed_receipts,png_signature
from native_fixture import NativeFixture,DEFAULT_BINARY,ROOT
from search_has_receipts import FIXTURE,CASES,validate,validate_stage

SPEC=CanvasCheck(FIXTURE,'SUPER_PLATINUM_SEARCH_HAS_CHECK',(),{},tuple(row[0] for row in CASES))

@contextmanager
def acknowledgments(evidence):
    owner=tempfile.TemporaryDirectory(prefix='sp-search-has-ack-')
    primary=None
    try:
        yield owner.name
    except BaseException as error:
        primary=error
        raise
    finally:
        try:owner.cleanup()
        except BaseException as error:
            detail=f'{type(error).__name__}: {error}'
            evidence.update(ok=False,ackCleanupError=detail)
            if primary is None:raise
            primary.add_note('Search acknowledgement cleanup also failed: '+detail)

def finalize(path,evidence,primary):
    # A canonical receipt appears only after complete serialization/write/close.
    # Both names are create-new. Retain the staged file intentionally as evidence;
    # there is no fallible unlink after publication and no partial success receipt.
    try:
        body=json.dumps(evidence,indent=2)+'\n'
        staged=path.with_suffix(path.suffix+'.staged')
        with staged.open('x',encoding='utf-8',newline='\n') as output:output.write(body)
        os.link(staged,path)
    except BaseException as error:
        detail=f'{type(error).__name__}: {error}'
        evidence.update(ok=False,finalizationError=detail)
        if primary is None:raise
        primary.add_note('Search receipt finalization also failed: '+detail)

def run(fixture,binary,captures):
    captures.mkdir(parents=True,exist_ok=True)
    names=[fixture+'.json',fixture+'.json.staged',fixture+'.log',fixture+'.png',fixture+'-failure.png',*[fixture+'-'+s+'.png' for s in SPEC.stages]]
    if any((captures/n).exists() for n in names):raise RuntimeError('Refusing existing Search evidence')
    evidence={'ok':False,'fixture':fixture,'platform':sys.platform,'stages':[]};native=None;primary=None
    try:
        with acknowledgments(evidence) as directory:
            native=NativeFixture(fixture,captures/(fixture+'.log'),binary,{SPEC.flag:'1','SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR':directory})
            with audited_fixture(native,evidence) as audit:
                deadline=time.monotonic()+180;seen=0;sampled=0
                while True:
                    native.require_running()
                    if time.monotonic()-sampled>=.2:audit.checkpoint('running');sampled=time.monotonic()
                    for stage in native.receipts(FIXTURE+'-stage: ')[seen:]:
                        first=None if not evidence['stages'] else evidence['stages'][0]['native']['revision']
                        validate_stage(stage,seen,fixture,first)
                        evidence['stages'].append(capture_stage(native,SPEC,fixture,stage,seen,captures,Path(directory),audit));seen+=1
                    receipts=native.receipts(FIXTURE+'-check: ')
                    if receipts:
                        evidence['terminalReceipts']=receipts
                        if len(receipts)!=1:raise RuntimeError('Expected one Search terminal')
                        evidence['native']=receipts[0];validate(fixture,receipts[0])
                        if seen!=len(SPEC.stages):raise RuntimeError('Search omitted captures')
                        if [{k:v for k,v in row.items() if k!='screenshot'} for row in evidence['stages']]!=receipts[0]['records']:raise RuntimeError('Captured Search states differ')
                        break
                    if time.monotonic()>=deadline:raise TimeoutError('Search terminal timed out')
                    time.sleep(.05)
                screenshot=captures/(fixture+'.png');audit.capture(screenshot,'final');evidence['screenshot']=str(screenshot)
            validate_closed(SPEC,fixture,native.log,evidence['stages'],evidence['native'])
            validate(fixture,evidence['native'])
            for capture in evidence['captures']:png_signature(Path(capture['path']),evidence.get('expectedPngDimensions'))
            evidence.update(closedLogRevalidated=True,ok=True)
    except BaseException as error:
        primary=error;evidence.update(ok=False,error=f'{type(error).__name__}: {error}');raise
    finally:
        if native is not None and native.log.exists():
            try:
                evidence['closedStageReceipts']=closed_receipts(native.log,FIXTURE+'-stage: ')
                evidence['closedTerminalReceipts']=closed_receipts(native.log,FIXTURE+'-check: ')
            except Exception as error:
                evidence.update(ok=False,closedLogError=str(error))
                if primary is not None:primary.add_note('Search closed-log collection also failed: '+str(error))
        finalize(captures/(fixture+'.json'),evidence,primary)
    if not evidence['ok']:raise RuntimeError('Search final closed-log audit failed')

def main():
    if not sys.flags.utf8_mode or not sys.dont_write_bytecode:raise RuntimeError('Invoke Python with -X utf8 -B')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,default=ROOT/'tmp/agent-ui/search-has')
    parser.add_argument('--fixture',choices=(FIXTURE,FIXTURE+'-narrow'),action='append')
    args=parser.parse_args()
    for fixture in args.fixture or (FIXTURE,FIXTURE+'-narrow'):run(fixture,args.binary.resolve(),args.captures.resolve())
if __name__=='__main__':main()
