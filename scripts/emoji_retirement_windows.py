"""One-process diagnostic capabilities. Never opens the live app or a named event."""
import ctypes as C
from ctypes import wintypes as W
import json
import os
import struct
import sys
import time
from emoji_retirement_receipts import BYTES, MAGIC, CAPACITY, ROLES, require

class API:
    def __init__(self):
        require(sys.platform=='win32','Native Windows required')
        self.k=C.WinDLL('kernel32',use_last_error=True)
        self.u=C.WinDLL('user32',use_last_error=True)
        signatures={
            'CreateEventW':([C.c_void_p,W.BOOL,W.BOOL,W.LPCWSTR],W.HANDLE),
            'CreateFileMappingW':([W.HANDLE,C.c_void_p,W.DWORD,W.DWORD,W.DWORD,W.LPCWSTR],W.HANDLE),
            'MapViewOfFile':([W.HANDLE,W.DWORD,W.DWORD,W.DWORD,C.c_size_t],C.c_void_p),
            'UnmapViewOfFile':([C.c_void_p],W.BOOL),
            'GetCurrentProcess':([],W.HANDLE),
            'OpenProcess':([W.DWORD,W.BOOL,W.DWORD],W.HANDLE),
            'GetProcessTimes':([W.HANDLE]+[C.POINTER(W.FILETIME)]*4,W.BOOL),
            'DuplicateHandle':([W.HANDLE,W.HANDLE,W.HANDLE,C.POINTER(W.HANDLE),W.DWORD,W.BOOL,W.DWORD],W.BOOL),
            'CloseHandle':([W.HANDLE],W.BOOL),'SetEvent':([W.HANDLE],W.BOOL),
        }
        for name,(args,result) in signatures.items():
            fn=getattr(self.k,name);fn.argtypes=args;fn.restype=result
        self.u.PostMessageW.argtypes=[W.HWND,W.UINT,W.WPARAM,W.LPARAM];self.u.PostMessageW.restype=W.BOOL
        self.u.GetWindowThreadProcessId.argtypes=[W.HWND,C.POINTER(W.DWORD)];self.u.GetWindowThreadProcessId.restype=W.DWORD
        self.u.IsWindow.argtypes=[W.HWND];self.u.IsWindow.restype=W.BOOL

    def checked(self,value,label):
        if not value:raise OSError(C.get_last_error(),label)
        return value

    def event(self):return self.checked(self.k.CreateEventW(None,True,False,None),'CreateEventW')
    def mapping(self):return self.checked(self.k.CreateFileMappingW(C.c_void_p(-1),None,4,0,BYTES,None),'CreateFileMappingW')
    def view(self,handle):return self.checked(self.k.MapViewOfFile(handle,6,0,0,BYTES),'MapViewOfFile')
    def initialize(self,view,header):
        C.memset(view,0,BYTES);data=struct.pack('<12Q',*header);C.memmove(view,data,len(data))
    def read(self,view):return C.string_at(view,BYTES)
    def open(self,pid):
        # Separate from read-only metrics: DUP_HANDLE + QUERY_LIMITED + SYNCHRONIZE.
        return self.checked(self.k.OpenProcess(0x40|0x1000|0x100000,False,pid),'OpenProcess diagnostic capability')
    def times(self,process):
        created,ended,kernel,user=(W.FILETIME() for _ in range(4))
        self.checked(self.k.GetProcessTimes(process,C.byref(created),C.byref(ended),C.byref(kernel),C.byref(user)),'GetProcessTimes')
        value=lambda f:(f.dwHighDateTime<<32)|f.dwLowDateTime
        return {'creation':value(created),'exit':value(ended)}
    def duplicate(self,handle,process,rights):
        child=W.HANDLE()
        self.checked(self.k.DuplicateHandle(self.k.GetCurrentProcess(),handle,process,C.byref(child),rights,False,0),'DuplicateHandle into child')
        return child.value
    def reclaim(self,process,child):
        local=W.HANDLE()
        self.checked(self.k.DuplicateHandle(process,child,self.k.GetCurrentProcess(),C.byref(local),0,False,3),'DuplicateHandle reclaim unpublished')
        self.close(local.value)
    def close(self,handle):self.checked(self.k.CloseHandle(handle),'CloseHandle')
    def unmap(self,view):self.checked(self.k.UnmapViewOfFile(view),'UnmapViewOfFile')
    def signal(self,event):self.checked(self.k.SetEvent(event),'SetEvent')
    def window_pid(self,hwnd):
        self.checked(self.u.IsWindow(hwnd),'Owned window missing');pid=W.DWORD()
        self.checked(self.u.GetWindowThreadProcessId(hwnd,C.byref(pid)),'GetWindowThreadProcessId')
        return pid.value
    def post_close(self,hwnd):self.checked(self.u.PostMessageW(hwnd,0x0010,0,0),'PostMessageW WM_CLOSE')

class Capabilities:
    def __init__(self,role,api=None):
        require(role in ROLES,'Unknown process role')
        self.api=api or API();self.role=role;self.event=None;self.mapping=None;self.view=None;self.process=None
        self.children=[];self.published=False;self.identity=None;self.audit=[];self.errors=[]
        # The sole unnamed B event exists before process launch.
        if role.startswith('B'):self.event=self.api.event()

    def attach(self,pid,creation,nonce):
        require(self.process is None,'Duplicate capability attachment')
        self.process=self.api.open(pid);self.identity={'pid':pid,'creation':creation,'nonce':nonce,'process':self.role}
        self.current()
        self.mapping=self.api.mapping();self.view=self.api.view(self.mapping)
        self.api.initialize(self.view,(MAGIC,1,CAPACITY,0,0,0,0,0,pid,int(nonce[:16],16),int(nonce[16:],16),ROLES.index(self.role)+1))
        mapping=self.api.duplicate(self.mapping,self.process,6);self.children.append(mapping)
        event=None
        if self.event:
            event=self.api.duplicate(self.event,self.process,0x100000);self.children.append(event)
        self.current()
        return dict(version=2,fixture='emoji-retirement-64000',**self.identity,
                    mapping=str(mapping),event=str(event) if event else None)

    def current(self):
        t=self.api.times(self.process)
        require(t['creation']==self.identity['creation'] and t['exit']==0,'Owned process identity retired')

    def publish(self,directory,ready):
        self.current();file=directory/'ready.json';tmp=directory/'ready.tmp'
        require(not file.exists() and not tmp.exists(),'Readiness already exists')
        with tmp.open('x',encoding='utf-8') as stream:
            json.dump(ready,stream,separators=(',',':'));stream.flush();os.fsync(stream.fileno())
        # Treat uncertainty as transferred BEFORE publication can become visible.
        self.published=True
        os.replace(tmp,file)
        self.audit.append({'kind':'published','children':len(self.children)})

    def close_request(self,hwnd,poll,clock=time.monotonic_ns,sleep=time.sleep):
        self.current();require(self.api.window_pid(hwnd)==self.identity['pid'],'Window belongs to another process')
        result={'request_ns':clock(),'posted':False,'signal_ns':None,'early_exit':False}
        self.audit.append(result)
        # No synchronous close send, waiting, capture or disk work before signal.
        try:
            self.api.post_close(hwnd);result['posted']=True;result['post_return_ns']=clock()
            if self.event:
                deadline=result['request_ns']+100_000_000
                while clock()<deadline:
                    if poll() is not None:result['early_exit']=True
                    remaining=deadline-clock()
                    if remaining>0:sleep(min(remaining/1e9,.005))
                if poll() is not None:result['early_exit']=True
                self.api.signal(self.event);result['signal_ns']=clock()
                require(100_000_000<=result['signal_ns']-result['request_ns']<=250_000_000,'B signal deadline')
                require(not result['early_exit'],'Process exited before B signal')
            return result
        except BaseException as error:
            result['error']=f'{type(error).__name__}: {error}'
            if self.event:
                try:self.api.signal(self.event);result['finally_signal_ns']=clock()
                except BaseException as cleanup:self.errors.append({'stage':'signal-after-post-failure','error':str(cleanup)})
            raise

    def snapshot_after_exit(self):
        times=self.api.times(self.process)
        require(times['creation']==self.identity['creation'] and times['exit']!=0,'Trace read before verified process exit')
        return self.api.read(self.view)

    def close(self):
        def attempt(stage,fn):
            try:fn()
            except BaseException as error:self.errors.append({'stage':stage,'error':f'{type(error).__name__}: {error}'})
        if self.event:attempt('finally-signal',lambda:self.api.signal(self.event))
        if self.children and not self.published:
            # Only the definitely-unpublished capability can be reclaimed.
            try:
                self.current()
            except BaseException as error:self.errors.append({'stage':'reclaim-identity','error':str(error)})
            else:
                for child in self.children:attempt('reclaim-unpublished',lambda child=child:self.api.reclaim(self.process,child))
        elif self.children:
            self.audit.append({'kind':'published-no-raw-reclaim','children':len(self.children)})
        self.children=[]
        if self.view:attempt('unmap',lambda:self.api.unmap(self.view));self.view=None
        for name in ('mapping','event','process'):
            handle=getattr(self,name)
            if handle:attempt('close-'+name,lambda handle=handle:self.api.close(handle));setattr(self,name,None)
