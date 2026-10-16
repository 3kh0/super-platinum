"""Closed, read-only SQLite and retained file identity verification."""
import copy
import ctypes
import json
import os
from pathlib import Path
import sqlite3
import stat
import sys


def identity(file):
    if sys.platform != 'win32':
        info=os.fstat(file.fileno())
        return [info.st_dev,info.st_ino]
    import msvcrt
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    call=kernel.GetFileInformationByHandle
    call.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint32)]
    call.restype=ctypes.c_int
    info=(ctypes.c_uint32*13)()
    if not call(msvcrt.get_osfhandle(file.fileno()),info):raise ctypes.WinError(ctypes.get_last_error())
    return [info[7],(info[11]<<32)|info[12]]


def validate(storage, receipt, h, *, temporary_root):
    require,same,keys,uuid4,unique,sha=(h[k] for k in ('require','same','keys','uuid4','unique_json','sha'))
    ticket=h['ticket'](receipt['ownedTicket'],receipt['account'])
    account=receipt['account'];target=receipt['target'];other='channel' if target=='thread' else 'thread'
    def regular(path,directory=False):
        info=path.lstat()
        require(not path.is_symlink() and not getattr(info,'st_file_attributes',0)&0x400
                and (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)), 'Linked/reparse/nonregular owned path')
        return info
    def chain(path):
        require(path.is_absolute() and '..' not in path.parts and '.' not in path.parts,'Noncanonical owned path')
        for parent in (path,*path.parents):regular(parent,True)
    chain(storage)
    same(storage.name,'protected-owned-send-'+receipt['nonce'],'Wrong handshake root')
    regular(storage/'owner.json')
    same(unique((storage/'owner.json').read_text(encoding='utf-8')),dict(version=1,nonce=receipt['nonce']),'Handshake owner changed')
    base=Path(temporary_root)
    chain(base)
    require(base.name.startswith("sp-fixture-"), "Wrong captured fixture temporary root")
    root=base/('super-platinum-upgrade-'+ticket['nonce'])
    chain(root)
    marker=root/'owner';db=root/('drafts-'+account+'.sqlite')
    same({p.name for p in root.iterdir()},{'owner',db.name},'Unexpected file/SQLite sidecar in closed owned root')
    require(regular(marker).st_size==36,'Invalid owner marker size')
    require(0<regular(db).st_size<32*1024*1024,'Invalid bounded owned database')
    before={p.name:sha(p) for p in (marker,db)}
    # Reject WAL/hot-journal states rather than ignoring them with immutable=1.
    with marker.open('rb') as owner,db.open('rb') as database:
        same(identity(owner),ticket['marker'],'Owned marker file identity changed')
        same(identity(database),ticket['database'],'Owned database file identity changed')
        same(owner.read(),account.encode('ascii'),'Owner marker bytes changed')
        header=database.read(100)
        require(header[:16]==b'SQLite format 3\0' and header[18:20]==b'\x01\x01','Unsupported WAL/database header')
        conn=sqlite3.connect(db.as_uri()+'?mode=ro',uri=True,timeout=.1)
        try:
            conn.execute('PRAGMA query_only=ON')
            same(conn.execute('PRAGMA integrity_check').fetchall(),[('ok',)],'Closed SQLite integrity failed')
            result=validate_connection(conn,receipt,h)
        finally:conn.close()
        same(identity(owner),ticket['marker'],'Owner replaced during read')
        same(identity(database),ticket['database'],'Database replaced during read')
    same({p.name for p in root.iterdir()},{'owner',db.name},'Read-only inspection created or found sidecar')
    same({p.name:sha(p) for p in (marker,db)},before,'Read-only inspection changed bytes')
    with marker.open('rb') as owner,db.open('rb') as database:
        same(identity(owner),ticket['marker'],'Marker path replaced during audit')
        same(identity(database),ticket['database'],'Database path replaced during audit')
    return dict(ok=True,database=str(db),root=str(root),ownedTicket=ticket,sha256=before,integrity='ok',**result)


def validate_connection(conn,receipt,h):
    same,require,keys,uuid4,unique=(h[k] for k in ('same','require','keys','uuid4','unique_json'))
    account=receipt['account'];target=receipt['target'];other='channel' if target=='thread' else 'thread'
    snapshots={v['stage']:v for v in receipt['snapshots']}
    final=snapshots['result']['durable']['scopes'];initial=snapshots['initial']['durable']['scopes']
    newer=snapshots['newer']['durable']['scopes'][target]
    operation=snapshots['result']['projection']['operation'];op=operation['operation'];intent=newer['row']['state']['consumption']
    committed=copy.deepcopy(newer);committed.update(operation['recorded'])
    def key(value):return json.dumps(value,sort_keys=True,separators=(',',':'))
    def destination(t):return key(h['scope'](account,t)['key'])
    def envelope(row):return h['STORAGE']['envelope'](row)
    def inventory_record(v):
        return dict(version=1,scope=v['scope'],local=v['local'],blocked=None,alternates=[],
                    row=None if v['row'] is None else [v['row']['revision'],envelope(v['row'])],origins=[],clock=v['clock'],generation=None)
    def decoded_record(v):
        if v.get('row') is not None:
            require(isinstance(v['row'],list) and len(v['row'])==2 and isinstance(v['row'][1],str),'Malformed persisted row pair')
            v['row'][1]=unique(v['row'][1])
        return v
    def mapped(rows):
        out={}
        for k,*rest in rows:
            k=key(unique(k));require(k not in out,'Duplicate semantic destination')
            out[k]=rest
        return out
    populated={'composer_drafts','composer_draft_sync','composer_draft_owner','composer_draft_adapter_format','composer_draft_continuity_format',
               'composer_draft_continuity','composer_draft_writer_lineage','composer_draft_local_clock','composer_draft_execution_result','composer_draft_protected_archive','composer_draft_migration_receipt'}
    empty={'composer_draft_blocked','composer_draft_alternates','composer_draft_reviewed_writers','composer_draft_known_read_limit','composer_draft_baseline_evidence',
           'composer_draft_recovery_editing','composer_draft_release_head','composer_draft_continuity_reads','composer_draft_recovery_owners','composer_draft_status_reads',
           'composer_draft_read_abandonment','composer_draft_read_late','upload_journal_identity','upload_receipts','sqlite_sequence'}
    same({r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")},populated|empty,'Unexpected closed database tables')
    same(conn.execute('PRAGMA user_version').fetchone(),(0,),'Unexpected schema version')
    same(conn.execute('PRAGMA application_id').fetchone(),(0,),'Unexpected application ID')
    for table in empty:same(conn.execute('SELECT * FROM "'+table+'"').fetchall(),[],'Unrelated persistent authority: '+table)
    for table,expected in [('composer_draft_owner',[(1,account)]),('composer_draft_adapter_format',[(1,1)]),('composer_draft_continuity_format',[(1,7)])]:
        same(conn.execute('SELECT * FROM '+table).fetchall(),expected,'Closed ownership/format differs')
    canonical=mapped(conn.execute('SELECT destination,content FROM composer_drafts').fetchall())
    canonical={k:unique(v[0]) for k,v in canonical.items()}
    same(canonical,{destination(t):final[t]['local'] for t in final},'Closed canonical input differs')
    protected=mapped(conn.execute('SELECT destination,storage_revision,format_version,state FROM composer_draft_sync').fetchall())
    protected={k:dict(revision=v[0],format=v[1],envelope=unique(v[2])) for k,v in protected.items()}
    row=final[other]['row']
    same(protected,{destination(other):dict(revision=row['revision'],format=7,envelope=envelope(row))},'Closed remaining managed scope differs')
    clocks=mapped(conn.execute('SELECT destination,revision,generation,writer_id,serial FROM composer_draft_local_clock').fetchall())
    require(set(clocks)=={destination(t) for t in final},'Wrong clock destination set')
    generation=clocks[destination(target)][1]
    require(uuid4(generation),'Missing actual issued local generation')
    editors=snapshots['result']['projection']['editors']
    same(clocks,{destination(other):[initial[other]['clock'],None,None,0],destination(target):[final[target]['clock'],generation,editors[target]['writerId'],3]},'Closed exact lease/clock differs')
    writers={}
    for writer,dest,raw in conn.execute('SELECT writer_id,destination,content FROM composer_draft_writer_lineage'):
        require(uuid4(writer) and writer not in writers,'Duplicate/invalid actual writer')
        v=unique(raw);same(v.get('scope',{}).get('key'),unique(dest),'Writer destination mismatch')
        if isinstance(v.get('last'),dict):decoded_record(v['last']['result'])
        writers[writer]=v
    expected_writers={}
    for t in final:
        v=newer if t==target else initial[t];serial=3 if t==target else 1;writer=editors[t]['writerId']
        expected_writers[writer]=dict(version=1,scope=v['scope'],writer=writer,origin=dict(Enrollment=initial[t]['row']['lineage']['enrollment']),processed=serial,
            last=dict(serial=serial,local=v['local'],direct=intent if t==target else None,continuation=None,preserved=False,result=inventory_record(v)))
    same(writers,expected_writers,'Closed historical writer receipts differ')
    archives=conn.execute('SELECT enrollment,team,content FROM composer_draft_protected_archive').fetchall()
    require(len(archives)==1,'Wrong archive count')
    enrollment=initial[target]['row']['lineage']['enrollment']
    same(archives[0][:2],(enrollment,'T1'),'Archive owner differs')
    archive=unique(archives[0][2]);archive['row']=unique(archive['row'])
    same(archive,dict(version=1,scope=committed['scope'],enrollment=enrollment,revision=committed['row']['revision'],row=envelope(committed['row']),
                      local=committed['local'],blocked=None,alternates=[],writer_floors=[]),'Archived terminal dictionary differs')
    journal=conn.execute('SELECT id,team,content FROM composer_draft_execution_result').fetchall()
    require(len(journal)==1,'Wrong exact execution journal count')
    jid='mutation:'+op['id'];same(journal[0][:2],(jid,'T1'),'Execution ID differs')
    entry=unique(journal[0][2]);decoded_record(entry['original'])
    same(entry,dict(version=1,id=jid,scope=committed['scope'],enrollment=enrollment,operation=op,
                    input=dict(Mutation=dict(Sent=dict(client_msg_id=op['client_msg_id'],draft_id=None,ts=h['ACK_TS']))),positive_accepted=True,
                    original=inventory_record(committed)),'Closed exact completed-call journal differs')
    teams=conn.execute('SELECT team,content FROM composer_draft_continuity').fetchall()
    require(len(teams)==1 and teams[0][0]=='T1','Unexpected team inventory')
    team=unique(teams[0][1]);require(uuid4(team.get('lease_id')),'Missing live retained stream identity')
    same(team,dict(epoch=2,lease_id=team['lease_id'],mode='Open',sequence=0,pending=None,last_interrupted_id=None,
                   gap_reason='new stream requires per-owner verification',read_retry_at_ms=0),'Stream normalized/revoked or unrelated work persisted')
    history=conn.execute('SELECT singleton,version,account,id,source,result FROM composer_draft_migration_receipt').fetchall()
    require(len(history)==1 and uuid4(history[0][3]),'Invalid migration receipt identity')
    same(history[0][:3],(1,1,account),'Migration account differs')
    same(unique(history[0][4]),None,'Fresh route fabricated migration source')
    same(unique(history[0][5]),dict(columns=['destination','storage_revision','format_version','state'],rows=[]),'Fresh migration imported extra state')
    return dict(canonical=canonical,protected=protected,clocks=clocks,writers=writers,archive=archive,execution=entry,continuity=team,
                migrationId=history[0][3],emptyTables=sorted(empty))
