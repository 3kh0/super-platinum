"""Pure protocol validation shared by native runner and mutation tests."""
import math

STAGES = ['initial', 'people', 'empty', 'unicode']
TIMES = ['eventEntryMs', 'queryDispatchMs', 'acceptedResultMs', 'raf1Ms', 'raf2Ms']


def require(value, message):
    if not value:
        raise ValueError(message)


def strict(a,b):
    if type(a) is not type(b): return False
    if isinstance(a,dict): return a.keys()==b.keys() and all(strict(a[k],b[k]) for k in a)
    if isinstance(a,list): return len(a)==len(b) and all(strict(x,y) for x,y in zip(a,b))
    return a==b


def number(value):
    return type(value) in (int,float) and math.isfinite(value) and value>=0


def reference_cases(reference,total):
    require(reference.get('ok') is True and reference['sha256_before']==reference['sha256_after'], 'Pure reference is not closed and passing')
    all_cases=[]
    for run in reference['runs']:
        inventory=next(i for i in run['inventories'] if i['total']==total)
        cases=[]
        for case in inventory['cases']:
            choices=[]
            for row in case['expected']:
                target=row['target'];kind=target['kind'];identity=target['id']
                token={'user':'<@{}>','channel':'<#{}>','group':'<!subteam^{}>','broadcast':'<!{}>'}[kind].format(identity)
                choices.append({'token':token,'label':row['label'],'detail':row['detail']})
            cases.append({'query':case['query'],'choices':choices})
        if all_cases: require(strict(cases,all_cases[0]), 'Pure reference ranking drift')
        all_cases.append(cases)
    require(len(all_cases)==reference['repeats'] and len(all_cases[0])==24,'Incomplete pure reference')
    return all_cases[0], inventory['counts']


def validate(receipt,identity,reference):
    require(receipt.get('ok') is True and strict(receipt.get('identity'),identity),'Wrong terminal identity')
    require(receipt.get('corpus')=='completion-directory-v1','Wrong corpus')
    require(receipt.get('noSession') is True and receipt.get('noTransport') is True,'Connected fixture')
    require(strict(receipt.get('captures'),STAGES),'Wrong capture stages')
    cases,counts=reference_cases(reference,identity['total'])
    require(strict(receipt.get('counts'),counts),'Native directory differs from pure reference')
    samples=receipt.get('samples')
    require(isinstance(samples,list) and len(samples)==identity['rounds']*24,'Missing/extra samples')
    previous=0
    for n,sample in enumerate(samples):
        repeat=n//24;index=((n%24)*7+repeat)%24;case=cases[index]
        require(type(sample.get('repeat')) is int and type(sample.get('case')) is int and (sample['repeat'],sample['case'])==(repeat,index),'Wrong sample order')
        require(sample.get('field')=='channel-composer' and sample.get('query')==case['query'],'Wrong field/query')
        require(strict(sample.get('choices'),case['choices']),'Actual native ranking differs from independent pure baseline')
        require(sample.get('status')==('No matches' if not case['choices'] else ''),'Wrong completion status')
        require(sample.get('inputTrusted') is False and sample.get('focused') is True and sample.get('connected') is True,'Unproven synthetic input/focus/mount')
        require(type(sample.get('seq')) is int and sample['seq']>previous,'Repeated or stale query sequence')
        previous=sample['seq']
        values=[sample.get(k) for k in TIMES]
        require(all(number(v) for v in values) and all(a<=b for a,b in zip(values,values[1:])) and values[-1]-values[0]<=5000,'Invalid or expired monotonic timings')
        require(number(sample.get('eventTimeStamp')),'Invalid event timestamp')
        viewport=sample.get('viewport',{})
        require(all(number(viewport.get(k)) and viewport[k]>0 for k in ('width','height')),'Invalid viewport')
        for name in ('fieldRect','popupRect'):
            rect=sample.get(name,{})
            require(all(number(rect.get(k)) for k in ('x','y','width','height')) and rect['width']>0 and rect['height']>0,'Invalid visible geometry')
            require(rect['x']+rect['width']<=viewport['width']+1 and rect['y']+rect['height']<=viewport['height']+1,'Completion geometry clipped')
    return samples


def durations(samples):
    result={}
    for s in samples:
        result.setdefault(s['query'],{'inputToQueryMs':[],'inputToResultMs':[],'inputToRaf2Ms':[]})
        for target,end in [('inputToQueryMs','queryDispatchMs'),('inputToResultMs','acceptedResultMs'),('inputToRaf2Ms','raf2Ms')]:
            result[s['query']][target].append(s[end]-s['eventEntryMs'])
    return result
