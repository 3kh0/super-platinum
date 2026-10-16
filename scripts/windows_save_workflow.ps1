# Separate opt-in workflow helper. Install under scripts/ only after root review.
# One bounded probe or one reviewed Value/Invoke action; no input injection.
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,UIAutomationClientsideProviders,WindowsBase
Add-Type -ReferencedAssemblies ([System.Windows.Automation.ClientSettings].Assembly.Location) -TypeDefinition @'
using System.Reflection;
public static class SaveWorkflowProxyBootstrap {
 [System.Runtime.CompilerServices.MethodImpl(System.Runtime.CompilerServices.MethodImplOptions.NoInlining)]
 public static void Register(AssemblyName provider) {
  System.Windows.Automation.ClientSettings.RegisterClientSideProviderAssembly(provider);
 }
}
'@
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class SaveWorkflowNative {
 [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd,uint command);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd,StringBuilder value,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd,StringBuilder value,int size);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 public static string Title(IntPtr hwnd) {var s=new StringBuilder(512);GetWindowText(hwnd,s,s.Capacity);return s.ToString();}
 public static string Class(IntPtr hwnd) {var s=new StringBuilder(256);GetClassName(hwnd,s,s.Capacity);return s.ToString();}
}
'@
$raw=$env:SP_SAVE_WORKFLOW_JSON
if([string]::IsNullOrWhiteSpace($raw) -or $raw.Length -gt 32768){throw 'Missing or oversized workflow request'}
$request=$raw | ConvertFrom-Json
if($request.mode -cnotin @('probe','set-path','invoke') -or $request.kind -cnotin @('save','overwrite') -or
   $request.nonce -cnotmatch '^[0-9a-f]{32}$' -or $request.pid -isnot [int] -or $request.pid -le 0 -or
   $request.creation -isnot [long] -or $request.creation -le 0 -or
   ($request.hwnd -isnot [int] -and $request.hwnd -isnot [long]) -or $request.hwnd -le 0){throw 'Invalid workflow owner'}
$fields=@('mode','kind','nonce','pid','creation','hwnd')
if($request.kind -ceq 'overwrite') {
 $fields+='saveHwnd'
 if(($request.saveHwnd -isnot [int] -and $request.saveHwnd -isnot [long]) -or
    $request.saveHwnd -le 0 -or $request.saveHwnd -eq $request.hwnd){throw 'Invalid Save owner HWND'}
}
if($request.mode -cne 'probe') {$fields+=@('window','target')}
if($request.mode -ceq 'set-path') {
 $fields+='allowedPath'
 if($request.kind -cne 'save'){throw 'Only Save filename editing is permitted'}
}
if($request.mode -ceq 'invoke') {
 $fields+='role'
 if(($request.kind -ceq 'save' -and $request.role -cnotin @('cancel','save')) -or
    ($request.kind -ceq 'overwrite' -and $request.role -cnotin @('decline','accept'))){throw 'Role is not permitted for this dialog'}
 if($request.role -ceq 'save') {$fields+=@('allowedPath','filenameTarget')}
}
$actualFields=@($request.PSObject.Properties.Name)
if($actualFields.Count -ne $fields.Count -or @($actualFields | Where-Object {$_ -cnotin $fields}).Count -ne 0){throw 'Unexpected request fields'}
$providerStatus=[ordered]@{ok=$false;nonce=$request.nonce;pid=$request.pid;scope='helper-process';uiAccessed=$false}
try {
 $provider=[UIAutomationClientsideProviders.UIAutomationClientSideProviders].Assembly
 $client=[System.Windows.Automation.ClientSettings].Assembly
 $providerName=$provider.GetName()
 $providerStatus.client=$client.FullName;$providerStatus.provider=$provider.FullName;$providerStatus.providerPath=$provider.Location
 if($providerName.Name -cne 'UIAutomationClientsideProviders' -or
    ([BitConverter]::ToString($providerName.GetPublicKeyToken()).Replace('-','').ToLowerInvariant()) -cne '31bf3856ad364e35' -or
    $providerName.Version -ne $client.GetName().Version){throw 'Unexpected built-in proxy assembly identity'}
 [SaveWorkflowProxyBootstrap]::Register($providerName)
 $providerStatus.tableCount=[UIAutomationClientsideProviders.UIAutomationClientSideProviders]::ClientSideProviderDescriptionTable.Count
 if($providerStatus.tableCount -le 0){throw 'Empty built-in proxy table'}
 $providerStatus.ok=$true
} catch {
 $providerStatus.error=$_.Exception.ToString()
 Write-Output ('save-workflow-provider: '+($providerStatus|ConvertTo-Json -Compress -Depth 5))
 throw
}
Write-Output ('save-workflow-provider: '+($providerStatus|ConvertTo-Json -Compress -Depth 5))
function Emit($record) {$record|ConvertTo-Json -Compress -Depth 12;[Console]::Out.Flush()}
function Same($left,$right) {
 # Descriptors must be replayed exactly, including probe property order.
 return ($left|ConvertTo-Json -Compress -Depth 12) -ceq ($right|ConvertTo-Json -Compress -Depth 12)
}
function Check-Process {
 $ownedProcess=Get-Process -Id $request.pid -ErrorAction Stop
 if($ownedProcess.StartTime.ToUniversalTime().ToFileTimeUtc() -ne $request.creation){throw 'Process identity changed'}
}
function Check-OwnedHwnd([IntPtr]$handle) {
 $windowProcess=[uint32]0
 if(-not [SaveWorkflowNative]::IsWindow($handle) -or -not [SaveWorkflowNative]::IsWindowVisible($handle)){throw 'Owned HWND disappeared'}
 [void][SaveWorkflowNative]::GetWindowThreadProcessId($handle,[ref]$windowProcess)
 if($windowProcess -ne $request.pid){throw 'HWND process mismatch'}
}
function Window-Description {
 Check-Process
 Check-OwnedHwnd $hwnd
 if([SaveWorkflowNative]::GetForegroundWindow() -ne $hwnd){throw 'Requested dialog is not foreground'}
 $title=[SaveWorkflowNative]::Title($hwnd)
 if($request.kind -ceq 'save' -and $title -cne 'Save original file - choose a new filename'){throw 'Unexpected Save title'}
 $chain=@()
 if($request.kind -ceq 'overwrite') {
  Check-OwnedHwnd ([IntPtr]$request.saveHwnd)
  if([SaveWorkflowNative]::Title([IntPtr]$request.saveHwnd) -cne 'Save original file - choose a new filename'){throw 'Unexpected owner Save title'}
  $owner=[SaveWorkflowNative]::GetWindow($hwnd,4)
  while($owner -ne [IntPtr]::Zero) {
   if($chain.Count -ge 8 -or $owner -eq $hwnd -or $owner.ToInt64() -in $chain){throw 'Invalid bounded GW_OWNER chain'}
   Check-OwnedHwnd $owner
   $chain+=,$owner.ToInt64()
   $owner=[SaveWorkflowNative]::GetWindow($owner,4)
  }
  if($request.saveHwnd -notin $chain){throw 'Overwrite is not owned by reviewed Save HWND'}
 }
 $rectangle=[SaveWorkflowNative+Rect]::new()
 if(-not [SaveWorkflowNative]::GetWindowRect($hwnd,[ref]$rectangle) -or
    $rectangle.Right -le $rectangle.Left -or $rectangle.Bottom -le $rectangle.Top){throw 'Invalid window geometry'}
 return [ordered]@{hwnd=$hwnd.ToInt64();title=$title;className=[SaveWorkflowNative]::Class($hwnd);
  rect=@($rectangle.Left,$rectangle.Top,($rectangle.Right-$rectangle.Left),($rectangle.Bottom-$rectangle.Top));ownerChain=$chain}
}
function Describe($element) {
 $current=$element.Current;$invokePattern=$null;$valuePattern=$null
 $invoke=$element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$invokePattern)
 $value=$element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern,[ref]$valuePattern)
 $readOnly=$null
 if($value){$readOnly=([System.Windows.Automation.ValuePattern]$valuePattern).Current.IsReadOnly}
 $rectangle=$current.BoundingRectangle
 return [ordered]@{runtimeId=@($element.GetRuntimeId());processId=$current.ProcessId;nativeHwnd=$current.NativeWindowHandle;
  automationId=$current.AutomationId;name=$current.Name;controlType=$current.ControlType.ProgrammaticName;className=$current.ClassName;
  enabled=$current.IsEnabled;offscreen=$current.IsOffscreen;invoke=$invoke;value=$value;readOnly=$readOnly;
  rect=@($rectangle.X,$rectangle.Y,$rectangle.Width,$rectangle.Height)}
}
function Find-Reviewed($descriptor) {
 if($null -eq $descriptor){throw 'Missing reviewed descriptor'}
 $matches=@($elements | Where-Object {
  ($_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -or
   $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit) -and (Same (Describe $_) $descriptor)
 })
 if($matches.Count -ne 1){throw 'Reviewed control is absent or ambiguous'}
 return $matches[0]
}
function Check-Target($element,$descriptor,[string]$controlType) {
 $current=Describe $element
 if(-not (Same $current $descriptor) -or $current.processId -ne $request.pid -or
    $current.controlType -cne $controlType -or -not $current.enabled -or $current.offscreen -or
    $current.rect[2] -le 0 -or $current.rect[3] -le 0){throw 'Reviewed control changed or is unavailable'}
 $point=[System.Windows.Point]::new(0,0)
 if(-not $element.TryGetClickablePoint([ref]$point)){throw 'Reviewed control has no visible point'}
 $hit=[System.Windows.Automation.AutomationElement]::FromPoint($point)
 if(-not [System.Windows.Automation.Automation]::Compare($element,$hit)){throw 'Reviewed control point is covered'}
 if(-not (Same (Window-Description) $request.window)){throw 'Reviewed dialog changed before action'}
 if(-not (Same (Describe $element) $descriptor)){throw 'Reviewed control changed during hit check'}
}
function Check-PrivatePath {
 if($request.allowedPath -isnot [string]){throw 'Missing frozen private path'}
 $repository=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
 if((Split-Path $PSScriptRoot -Leaf) -cne 'scripts' -or
    -not (Test-Path -LiteralPath (Join-Path $repository 'Cargo.toml') -PathType Leaf) -or
    -not (Test-Path -LiteralPath (Join-Path $repository 'AGENTS.md') -PathType Leaf)){throw 'Helper is not installed in repository scripts'}
 $directory=Join-Path $repository ('tmp\original-save-workflow-'+$request.nonce+'\downloads')
 $allowed=@((Join-Path $directory ('preserve-'+$request.nonce+'.bin')),(Join-Path $directory ('saved-'+$request.nonce+'.bin')))
 if($request.allowedPath -cnotin $allowed -or [IO.Path]::GetFullPath($request.allowedPath) -cne $request.allowedPath){throw 'Path is not one of the frozen private destinations'}
 $part=Get-Item -LiteralPath $directory -Force -ErrorAction Stop
 while($null -ne $part) {
  if(-not ($part.Attributes -band [IO.FileAttributes]::Directory) -or ($part.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Private directory has an unsafe ancestor'}
  $part=$part.Parent
 }
 if(Test-Path -LiteralPath $request.allowedPath) {
  $file=Get-Item -LiteralPath $request.allowedPath -Force
  if($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Private destination is not an ordinary file'}
 }
}
function Filename-Pattern($element,$descriptor) {
 Check-Target $element $descriptor 'ControlType.Edit'
 if(-not $descriptor.value -or $descriptor.readOnly -ne $false){throw 'Filename must expose a writable ValuePattern'}
 return [System.Windows.Automation.ValuePattern]$element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
}
$hwnd=[IntPtr]$request.hwnd
Check-Process
$prior=[SaveWorkflowNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
if($prior -eq [IntPtr]::Zero){throw 'DPI context unavailable'}
try {
 $window=Window-Description
 $root=[System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
 if($root.Current.ProcessId -ne $request.pid){throw 'UIA root process mismatch'}
 $elements=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 if($elements.Count -gt 512){throw 'Dialog subtree exceeds bounded discovery'}
 $controls=@()
 foreach($element in $elements) {
  if($element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button -or
     $element.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit){$controls+=,(Describe $element)}
 }
 if($controls.Count -gt 80){throw 'Too many dialog controls'}
 if(-not (Same $window (Window-Description))){throw 'Dialog changed during inventory'}
 if($request.mode -ceq 'probe') {
  Emit ([ordered]@{ok=$true;mode='probe';kind=$request.kind;nonce=$request.nonce;pid=$request.pid;creation=$request.creation;window=$window;controls=$controls})
  exit 0
 }
 if(-not (Same $window $request.window)){throw 'Reviewed dialog no longer matches'}
 $selected=Find-Reviewed $request.target
 if($request.mode -ceq 'set-path') {
  Check-PrivatePath
  $pattern=Filename-Pattern $selected $request.target
  Emit ([ordered]@{kind='before-set';nonce=$request.nonce;pid=$request.pid;creation=$request.creation;hwnd=$request.hwnd;target=$request.target;allowedPath=$request.allowedPath;count=1})
  Check-PrivatePath
  Check-Target $selected $request.target 'ControlType.Edit'
  $pattern.SetValue($request.allowedPath)
  if($pattern.Current.Value -cne $request.allowedPath){throw 'Filename readback differs from frozen full path'}
  Check-Target $selected $request.target 'ControlType.Edit'
  Check-PrivatePath
  Emit ([ordered]@{kind='after-set';nonce=$request.nonce;pid=$request.pid;creation=$request.creation;hwnd=$request.hwnd;allowedPath=$request.allowedPath;valueEqualsAllowedPath=$true;count=1})
  exit 0
 }
 $names=@{cancel='Cancel';save='Save';decline='No';accept='Yes'}
 $expectedName=$names[$request.role]
 if($request.target.name -cne $expectedName -and $request.target.name -cne ('&'+$expectedName)){throw 'Unexpected button locale or semantic name'}
 Check-Target $selected $request.target 'ControlType.Button'
 if(-not $request.target.invoke){throw 'Reviewed button does not support Invoke'}
 if($request.role -ceq 'save') {
  Check-PrivatePath
  $filename=Find-Reviewed $request.filenameTarget
  $filenamePattern=Filename-Pattern $filename $request.filenameTarget
  if($filenamePattern.Current.Value -cne $request.allowedPath){throw 'Save filename is not the frozen full path'}
 }
 Emit ([ordered]@{kind='before-invoke';nonce=$request.nonce;pid=$request.pid;creation=$request.creation;hwnd=$request.hwnd;dialogKind=$request.kind;role=$request.role;target=$request.target;count=1})
 if($request.role -ceq 'save') {
  Check-PrivatePath
  Check-Target $filename $request.filenameTarget 'ControlType.Edit'
  if($filenamePattern.Current.Value -cne $request.allowedPath){throw 'Save filename changed before invocation'}
 }
 Check-Target $selected $request.target 'ControlType.Button'
 ([System.Windows.Automation.InvokePattern]$selected.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)).Invoke()
 # Save can block here while a separately owned helper resolves overwrite.
 Emit ([ordered]@{kind='invoke-returned';nonce=$request.nonce;pid=$request.pid;creation=$request.creation;hwnd=$request.hwnd;dialogKind=$request.kind;role=$request.role;count=1})
} finally {[void][SaveWorkflowNative]::SetThreadDpiAwarenessContext($prior)}
