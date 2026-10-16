# One owned Save dialog: read-only inventory, or one reviewed Cancel invocation.
# No filename edits, Save/overwrite action, clipboard or keyboard injection.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,UIAutomationClientsideProviders,WindowsBase
Add-Type -ReferencedAssemblies ([System.Windows.Automation.ClientSettings].Assembly.Location) -TypeDefinition @'
using System.Reflection;
public static class SaveDialogProxyBootstrap {
 // The Framework default proxy loader inspects its first external stack frame.
 // Preserve this typed frame: a PowerShell dynamic frame can have no ReflectedType.
 [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
 public static void Register(AssemblyName provider) {
  System.Windows.Automation.ClientSettings.RegisterClientSideProviderAssembly(provider);
 }
}
'@
Add-Type @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class SaveDialogNative {
 public delegate bool EnumProc(IntPtr hwnd,IntPtr data);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback,IntPtr data);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder value,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder value,int size);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 public static IntPtr[] Owned(uint pid) {
  var list=new List<IntPtr>(); EnumWindows((h,d)=>{uint p;GetWindowThreadProcessId(h,out p);if(p==pid&&IsWindowVisible(h))list.Add(h);return true;},IntPtr.Zero);return list.ToArray();
 }
 public static string Title(IntPtr hwnd) {var s=new StringBuilder(512);GetWindowText(hwnd,s,s.Capacity);return s.ToString();}
 public static string Class(IntPtr hwnd) {var s=new StringBuilder(256);GetClassName(hwnd,s,s.Capacity);return s.ToString();}
}
'@
$request=$env:SP_SAVE_DIALOG_JSON | ConvertFrom-Json
if ($request.mode -cnotin @('probe','cancel') -or $request.nonce -cnotmatch '^[0-9a-f]{32}$' -or $request.pid -le 0 -or $request.creation -le 0) {throw 'Invalid diagnostic owner'}
$providerStatus=[ordered]@{ok=$false;nonce=$request.nonce;pid=[int]$request.pid;scope='helper-process';uiAccessed=$false}
try {
 $provider=[UIAutomationClientsideProviders.UIAutomationClientSideProviders].Assembly
 $client=[System.Windows.Automation.ClientSettings].Assembly
 $providerName=$provider.GetName()
 $providerStatus.client=$client.FullName;$providerStatus.provider=$provider.FullName;$providerStatus.providerPath=$provider.Location
 if($providerName.Name -cne 'UIAutomationClientsideProviders' -or
    ([BitConverter]::ToString($providerName.GetPublicKeyToken()).Replace('-','').ToLowerInvariant()) -cne '31bf3856ad364e35' -or
    $providerName.Version -ne $client.GetName().Version){throw 'Unexpected built-in proxy assembly identity'}
 [SaveDialogProxyBootstrap]::Register($providerName)
 $providerStatus.tableCount=[UIAutomationClientsideProviders.UIAutomationClientSideProviders]::ClientSideProviderDescriptionTable.Count
 if($providerStatus.tableCount -le 0){throw 'Empty built-in proxy table'}
 $providerStatus.ok=$true
} catch {
 $providerStatus.error=$_.Exception.ToString()
 Write-Output ('save-dialog-provider: '+($providerStatus|ConvertTo-Json -Compress -Depth 5))
 throw
}
Write-Output ('save-dialog-provider: '+($providerStatus|ConvertTo-Json -Compress -Depth 5))
function Check-Process {
 $process=Get-Process -Id $request.pid -ErrorAction Stop
 if ($process.StartTime.ToUniversalTime().ToFileTimeUtc() -ne $request.creation) {throw 'Process identity changed'}
}
function Same($a,$b) { return ($a|ConvertTo-Json -Compress -Depth 10) -ceq ($b|ConvertTo-Json -Compress -Depth 10) }
function Describe($e) {
 $c=$e.Current;$pattern=$null
 $invoke=$e.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)
 $r=$c.BoundingRectangle
 return [ordered]@{runtimeId=@($e.GetRuntimeId());automationId=$c.AutomationId;name=$c.Name;controlType=$c.ControlType.ProgrammaticName;className=$c.ClassName;enabled=$c.IsEnabled;offscreen=$c.IsOffscreen;invoke=$invoke;rect=@($r.X,$r.Y,$r.Width,$r.Height)}
}
Check-Process
$prior=[SaveDialogNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
if($prior -eq [IntPtr]::Zero){throw 'DPI context unavailable'}
try {
 $found=@([SaveDialogNative]::Owned([uint32]$request.pid) | Where-Object { [SaveDialogNative]::Title($_) -ceq 'Save original file - choose a new filename' })
 if($found.Count -ne 1){throw 'Expected exactly one owned Save title'}
 $hwnd=$found[0]
 if([SaveDialogNative]::GetForegroundWindow() -ne $hwnd){throw 'Save dialog is not foreground'}
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
 if($root.Current.ProcessId -ne $request.pid){throw 'UIA root process mismatch'}
 $rect=[SaveDialogNative+Rect]::new()
 if(-not [SaveDialogNative]::GetWindowRect($hwnd,[ref]$rect)){throw 'Window geometry unavailable'}
 $elements=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 if($elements.Count -gt 512){throw 'Dialog subtree exceeds bounded discovery'}
 $controls=@();$selected=$null
 foreach($e in $elements) {
  if($e.Current.ControlType -ne [System.Windows.Automation.ControlType]::Button -and $e.Current.ControlType -ne [System.Windows.Automation.ControlType]::Edit){continue}
  # Never collect file lists, navigation-tree contents, edit values or shell history.
  $description=Describe $e
  $controls+=,$description
  if($request.mode -ceq 'cancel' -and (Same $description $request.target)) {
   if($null -ne $selected){throw 'Ambiguous reviewed control'};$selected=$e
  }
 }
 if($controls.Count -gt 80){throw 'Too many dialog controls'}
 $observation=[ordered]@{ok=$true;mode=$request.mode;nonce=$request.nonce;pid=[int]$request.pid;creation=[long]$request.creation;hwnd=$hwnd.ToInt64();title=[SaveDialogNative]::Title($hwnd);className=[SaveDialogNative]::Class($hwnd);rect=@($rect.Left,$rect.Top,($rect.Right-$rect.Left),($rect.Bottom-$rect.Top));controls=$controls}
 if($request.mode -ceq 'probe') {$observation | ConvertTo-Json -Compress -Depth 10;exit 0}
 if($hwnd.ToInt64() -ne $request.hwnd -or $null -eq $selected){throw 'Reviewed HWND/control no longer matches'}
 $before=Describe $selected
 if($before.name -cne 'Cancel' -or $before.controlType -cne 'ControlType.Button' -or -not $before.enabled -or $before.offscreen -or -not $before.invoke){throw 'Only an observed enabled English Cancel button is permitted'}
 $point=[System.Windows.Point]::new(0,0)
 if(-not $selected.TryGetClickablePoint([ref]$point)){throw 'Cancel has no visible point'}
 $hit=[System.Windows.Automation.AutomationElement]::FromPoint($point)
 if(-not [System.Windows.Automation.Automation]::Compare($selected,$hit)){throw 'Cancel point is covered'}
 Check-Process
 if([SaveDialogNative]::GetForegroundWindow() -ne $hwnd -or -not (Same (Describe $selected) $request.target)){throw 'Reviewed Cancel changed before invocation'}
 [ordered]@{kind='before-invoke';nonce=$request.nonce;pid=[int]$request.pid;hwnd=$hwnd.ToInt64();target=$before;action='cancel';count=1}|ConvertTo-Json -Compress -Depth 10
 ([System.Windows.Automation.InvokePattern]$selected.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
 [ordered]@{kind='invoke-returned';nonce=$request.nonce;pid=[int]$request.pid;hwnd=$hwnd.ToInt64();action='cancel';count=1}|ConvertTo-Json -Compress
} finally { [void][SaveDialogNative]::SetThreadDpiAwarenessContext($prior) }
