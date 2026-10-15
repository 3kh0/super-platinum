"""Independent overhead receipt arithmetic; separate from lifecycle evidence."""
import math
from emoji_retirement_receipts import _layout, integer, require

def validate(receipt,disabled,enabled,manifest):
    require(receipt['schema']==1 and type(receipt['schema']) is int and receipt['kind']=='empty-wrapper-overhead','Overhead schema')
    require(receipt['manifest_sha256']==manifest and receipt['nonce']=='0'*32 and receipt['process']=='W1','Overhead provenance')
    identity={key:receipt[key] for key in ('pid','nonce','process')}
    off=_layout(disabled,identity);on=_layout(enabled,identity)
    require(off['header'][5]==on['header'][5] and off['events']==[],'Separate disabled ledger was not empty/same-thread')
    events=on['events'];require(len(events)==35000,'Exact enabled event inventory')
    children={};previous=0
    for e in events:
        require(e['role']==1 and e['thread']==on['header'][5] and e['at']>=previous,'Overhead actual serial thread/clock')
        previous=e['at']
        require(e['context']==0 and e['write']==0 and e['mount']==0 and e['action']==0 and e['related']==0,'Overhead context leaked')
        require(e['kind'] in (1,2,3,4,5,10,11),'Unexpected overhead event')
        require(e['child']==(0 if e['kind'] in (10,11) else 1),'Overhead child identity')
        children.setdefault(e['generation'],[]).append(e['kind'])
    require(set(children)=={(ticket<<2)|1 for ticket in range(1,5001)},'Missing/repeated enabled generation')
    require(all(kinds==[10,1,11,2,3,4,5] for kinds in children.values()),'Missing/duplicated overhead phases')
    blocks=receipt['blocks'];require(len(blocks)==100,'Exact 100 blocks required')
    arms={False:[],True:[]}
    for i,b in enumerate(blocks):
        require(set(b)=={'block','enabled','drops','nanoseconds','record_start_ns','record_end_ns'},'Block shape')
        require(integer(b['block']) and b['block']==i and type(b['enabled']) is bool and b['enabled']==(i%2==1),'Alternating fixed block order')
        require(integer(b['drops']) and b['drops']==100 and integer(b['nanoseconds']),'Block counts/clock')
        require(integer(b['record_start_ns']) and integer(b['record_end_ns'])
                and b['record_end_ns']-b['record_start_ns']>=b['nanoseconds'],'Actual timer bracket')
        if b['enabled']:
            first=(i//2)*100+1;last=first+99
            selected=[e for e in events if first<=e['generation']>>2<=last]
            require(len(selected)==700,'Block generation inventory')
            require(all(e['at']<=b['record_start_ns'] if e['kind'] in (10,1,11)
                        else b['record_start_ns']<=e['at']<=b['record_end_ns'] for e in selected),'Construction inside timing or drop outside actual bracket')
        arms[b['enabled']].append(b['nanoseconds'])
    medians={}
    for arm,values in arms.items():
        require(len(values)==50,'Fifty blocks per arm')
        values.sort();medians[arm]=(float(values[24])+float(values[25]))/200.0
    difference=medians[True]-medians[False]
    require(all(type(receipt[k]) in (int,float) and math.isfinite(receipt[k]) for k in
        ('disabled_median_ns_per_drop','enabled_median_ns_per_drop','difference_ns_per_drop')),'Overhead median number types')
    require(receipt['disabled_median_ns_per_drop']==medians[False] and receipt['enabled_median_ns_per_drop']==medians[True]
            and receipt['difference_ns_per_drop']==difference,'Overhead median arithmetic')
    require(type(receipt['threshold_ns_per_drop']) is int and receipt['threshold_ns_per_drop']==10000,'Fixed overhead threshold')
    require(all(integer(receipt[k]) for k in ('enabled_child_events','enabled_total_events','disabled_total_events')),'Overhead count types')
    require((receipt['enabled_child_events'],receipt['enabled_total_events'],receipt['disabled_total_events'])==(25000,35000,0),'Overhead counts changed')
    require(receipt['complete'] is True and receipt['ok'] is (difference<=10000),'Overhead status mismatch')
    require(receipt['ok'] is True,'Observer overhead exceeds the frozen gate')
    return {'ok':True,'drops':10000,'blocks':100,'difference_ns_per_drop':difference,'threshold_ns_per_drop':10000,
            'excluded_from_native_evidence':True}
