"""Closed owned native channel/thread selected-map publication and retirement."""
import argparse
from pathlib import Path
import canvas_native
from native_fixture import DEFAULT_BINARY
import channel_thread_emoji as oracle

SPEC=canvas_native.CanvasCheck(fixture=oracle.PREFIX,flag=oracle.FLAG,required=(),counts={},stages=oracle.STAGES)
_validate=canvas_native.validate
_context=canvas_native.context_valid
_closed=canvas_native.validate_closed

def context(spec,fixture,receipt):
    if spec is SPEC:return fixture in oracle.FIXTURES and oracle.context(receipt)
    return _context(spec,fixture,receipt)

def validate(spec,fixture,receipt):
    oracle.require(spec is SPEC and fixture in oracle.FIXTURES,'Unexpected channel/thread fixture')
    _validate(spec,fixture,receipt)
    oracle.validate(receipt,fixture)

def closed(spec,fixture,log,stages,terminal):
    _closed(spec,fixture,log,stages,terminal)
    oracle.require([{k:v for k,v in s.items() if k!='screenshot'} for s in stages]==terminal.get('stages'),'Original staged/terminal receipt differed')

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=DEFAULT_BINARY)
    parser.add_argument('--captures',type=Path,required=True)
    parser.add_argument('--fixture',action='append',choices=oracle.FIXTURES)
    args=parser.parse_args()
    canvas_native.context_valid=context;canvas_native.validate=validate;canvas_native.validate_closed=closed
    for fixture in args.fixture or oracle.FIXTURES:
        canvas_native.run(SPEC,fixture,args.binary.resolve(),args.captures.resolve())

if __name__=='__main__':main()
