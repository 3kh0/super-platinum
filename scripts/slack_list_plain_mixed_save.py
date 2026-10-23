"""Two isolated native variants sharing the original owned plain-edit harness."""
import runpy
from functools import lru_cache
from pathlib import Path
from slack_list_plain_member_save_facts import require
CASES=('own-edit-plain-mixed-tie','own-edit-plain-mixed-group')
STAGES={case:('draft','editing','saving','saved','readback') for case in CASES}

@lru_cache(maxsize=4)
def configured(case,history):
    require(case in CASES,'Unknown mixed case')
    return runpy.run_path(str(Path(__file__).with_name('slack_list_plain_mixed_save_impl.py')),init_globals={'CASE':case,'HISTORY':history})

def validate(fixture,receipt,directory,*,expected_case=None,**kw):
    case=expected_case or receipt.get('case');require(case in CASES,'Unknown mixed terminal')
    history=receipt['stages'][0]['plain']['mixed']['historyWire']
    return configured(case,history)['validate'](fixture,receipt,directory,expected_case=case,**kw)

def validate_closed(snapshot,receipt):
    configured(receipt['case'],receipt['stages'][0]['plain']['mixed']['historyWire'])['validate_closed'](snapshot,receipt)

def validator(case):
    require(case in CASES,'Unknown mixed case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
