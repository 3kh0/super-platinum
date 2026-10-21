"""Read canonical first-file Canvas roots through the unchanged owned native harness."""
import canvas_native
from canvas_native import CanvasCheck, main
import canvas_canonical_file_roots as oracle
SPEC=CanvasCheck(fixture='canvas-canonical-file-roots',flag='SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK',
 required=('canonicalDisplay','firstOnly','rawPreserved','noDiscardedMedia','readOnly'),
 counts={'requestCount':9},stages=oracle.STAGES,
 private_files=(('SUPER_PLATINUM_CANVAS_REPLY_DB','replies.sqlite'),))
_base=canvas_native.validate
_closed=canvas_native.validate_closed
def validate(spec,fixture,receipt):
 _base(spec,fixture,receipt);oracle.validate(receipt)
def validate_closed(spec,fixture,log,stages,terminal):
 _closed(spec,fixture,log,stages,terminal)
 if not oracle.same([{k:v for k,v in s.items() if k!='screenshot'} for s in stages],terminal.get('stages')):
  raise RuntimeError('Original canonical display capture changed')
if __name__=='__main__':
 canvas_native.validate=validate;canvas_native.validate_closed=validate_closed;main(SPEC,__doc__)
