# Invoke exactly one isolated fixture toast through UI Automation. No cursor,
# notification-center, preference, registration, or unrelated notification edits.
# Arguments are data in SP_TOAST_ACTIVATION_JSON, never PowerShell source.
# API contracts: https://learn.microsoft.com/dotnet/api/system.windows.automation.automationelement.trygetcurrentpattern
# https://learn.microsoft.com/dotnet/api/system.windows.automation.invokepattern.invoke
# https://learn.microsoft.com/dotnet/api/system.windows.automation.automationelement.fromhandle
# https://learn.microsoft.com/dotnet/api/system.windows.automation.automationelement.frompoint
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumwindows
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-iswindowvisible
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getsystemmetrics
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-windowfrompoint
# https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getclassnamew
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, WindowsBase
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Text;
public static class ToastProbeNative {
    [StructLayout(LayoutKind.Sequential)] private struct Point { public int X; public int Y; }
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr value);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
    private delegate bool EnumWindowProc(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowProc callback, IntPtr data);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")] private static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, ExactSpelling=true)] private static extern int GetClassNameW(IntPtr hwnd, StringBuilder name, int count);
    public static IntPtr WindowAt(int x, int y) { return WindowFromPoint(new Point { X=x, Y=y }); }
    public static string ClassName(IntPtr hwnd) {
        var name = new StringBuilder(512);
        return GetClassNameW(hwnd, name, name.Capacity) > 0 ? name.ToString() : "";
    }
    public static IntPtr[] VisibleWindows() {
        var windows = new List<IntPtr>();
        EnumWindows((hwnd, data) => { if (IsWindowVisible(hwnd)) windows.Add(hwnd); return true; }, IntPtr.Zero);
        return windows.ToArray();
    }
}
'@
$request = $env:SP_TOAST_ACTIVATION_JSON | ConvertFrom-Json
$mode = if ($null -eq $request.mode) { 'warm' } else { $request.mode }
$bodyPrefix = if ($mode -ceq 'cold') { 'Super Platinum cold activation check' } else { 'Super Platinum activation check' }
if ($request.nonce -cnotmatch '^[0-9a-f]{32}$' -or
    $mode -cnotin @('warm', 'cold') -or
    $request.body -cne "$bodyPrefix $($request.nonce)" -or
    $request.title -cne 'Super Platinum' -or
    $request.appName -cne 'Super Platinum Fixture' -or
    $request.app_id -cne 'com.echonet.superplatinum.fixture.notifications' -or
    $request.pid -le 0) { throw 'Invalid isolated fixture target' }
$previousDpi = [ToastProbeNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($previousDpi -eq [IntPtr]::Zero) { throw 'Physical-pixel DPI context unavailable' }
$trusted = @{}
$endpointTrust = @{}
$walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
$scope = [System.Windows.Automation.TreeScope]::Descendants
$all = [System.Windows.Automation.Condition]::TrueCondition

function Same-Element($left, $right) {
    return [System.Windows.Automation.Automation]::Compare($left, $right)
}
function Has-ExactName($element, [string]$name) {
    if ($element.Current.Name -ceq $name) { return $true }
    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty, $name)
    return $null -ne $element.FindFirst($scope, $condition)
}
function Trusted-Endpoint([int]$processId) {
    if ($processId -eq $request.pid -or $processId -le 0) { return $false }
    if ($trusted.ContainsKey($processId)) { return $trusted[$processId] }
    $trusted[$processId] = $false
    $endpointTrust[$processId] = @{processId=$processId; trusted=$false; reason='process-unavailable'}
    try {
        $process = Get-Process -Id $processId -ErrorAction Stop
        $path = $process.Path
        $leaf = [IO.Path]::GetFileName($path)
        $endpointTrust[$processId].executable = $leaf
        if ($leaf -notin @('ShellExperienceHost.exe', 'ShellHost.exe', 'explorer.exe') -or
            -not $path.StartsWith($env:SystemRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
            $endpointTrust[$processId].reason = 'not-allowed-windows-shell-host'
            return $false
        }
        $signature = Get-AuthenticodeSignature -LiteralPath $path
        $endpointTrust[$processId].signatureStatus = [string]$signature.Status
        $endpointTrust[$processId].microsoftSigner =
            $signature.SignerCertificate.Subject -match 'O=Microsoft Corporation(?:,|$)'
        $trusted[$processId] = $signature.Status -eq 'Valid' -and
            $endpointTrust[$processId].microsoftSigner
        $endpointTrust[$processId].trusted = $trusted[$processId]
        $endpointTrust[$processId].reason = if ($trusted[$processId]) { 'trusted' } else { 'signature-rejected' }
    } catch { return $false }
    return $trusted[$processId]
}
function Describe($element, [bool]$inspectSubtree = $true) {
    $current = $element.Current
    $rect = $current.BoundingRectangle
    $invokePattern = $null
    $invokeAvailable = $element.TryGetCurrentPattern(
        [System.Windows.Automation.InvokePattern]::Pattern, [ref]$invokePattern)
    # Other notification text is never recorded. Keep structural metadata useful
    # when a Windows version exposes no safe Invoke target.
    $name = if ($current.Name -cin @($request.body, $request.title, $request.appName)) {
        $current.Name
    } else { '<redacted>' }
    return [ordered]@{name=$name; automationId=$current.AutomationId; className=$current.ClassName;
        controlType=$current.ControlType.ProgrammaticName; processId=$current.ProcessId;
        runtimeId=@($element.GetRuntimeId()); enabled=$current.IsEnabled;
        offscreen=$current.IsOffscreen; rect=@($rect.X,$rect.Y,$rect.Width,$rect.Height);
        invokeAvailable=$invokeAvailable;
        exactAppNameInSubtree=($inspectSubtree -and (Has-ExactName $element $request.appName));
        exactTitleInSubtree=($inspectSubtree -and (Has-ExactName $element $request.title));
        exactBodyInSubtree=($inspectSubtree -and (Has-ExactName $element $request.body));
        subtreeInspected=$inspectSubtree}
}
function Describe-NativeWindow([IntPtr]$hwnd) {
    [uint32]$windowPid = 0
    $threadId = [ToastProbeNative]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    $accepted = Trusted-Endpoint $windowPid
    return @{hwnd=$hwnd.ToInt64(); processId=$windowPid; threadId=$threadId;
        className=[ToastProbeNative]::ClassName($hwnd); trusted=$accepted;
        trust=$endpointTrust[[int]$windowPid]}
}
function Visible-Rect($element) {
    $current = $element.Current
    $rect = $current.BoundingRectangle
    $left = [ToastProbeNative]::GetSystemMetrics(76)
    $top = [ToastProbeNative]::GetSystemMetrics(77)
    $right = $left + [ToastProbeNative]::GetSystemMetrics(78)
    $bottom = $top + [ToastProbeNative]::GetSystemMetrics(79)
    return $current.IsEnabled -and -not $current.IsOffscreen -and
        -not $rect.IsEmpty -and $rect.Width -gt 0 -and $rect.Height -gt 0 -and
        $rect.Left -ge $left -and $rect.Top -ge $top -and
        $rect.Right -le $right -and $rect.Bottom -le $bottom
}
function Target-UnderPoint($target, $point, $endpoint) {
    $bounds = $target.Current.BoundingRectangle
    if ($point.X -lt $bounds.Left -or $point.X -ge $bounds.Right -or
        $point.Y -lt $bounds.Top -or $point.Y -ge $bounds.Bottom) { return $null }
    $hit = [System.Windows.Automation.AutomationElement]::FromPoint($point)
    for ($depth = 0; $null -ne $hit -and $depth -lt 32; $depth++) {
        if (Same-Element $target $hit) { return @{method='UIAutomation.FromPoint'; accepted=$true} }
        $hit = $walker.GetParent($hit)
    }
    # Observed on this Windows build: UIA FromPoint returns the desktop while
    # native WindowFromPoint returns the toast's signed ShellExperienceHost HWND.
    # This fallback verifies that exact already-inspected provider, not merely
    # an arbitrary window from the same process, and never clicks the point.
    $nativeHit = Describe-NativeWindow ([ToastProbeNative]::WindowAt([int]$point.X, [int]$point.Y))
    if (-not $nativeHit.trusted -or $nativeHit.processId -ne $target.Current.ProcessId -or
        -not $nativeHitProviders.ContainsKey($nativeHit.hwnd)) { return $null }
    $provider = $nativeHitProviders[$nativeHit.hwnd]
    if (-not $provider.admitted -or $provider.nativeProcessId -ne $nativeHit.processId -or
        -not [System.Windows.Automation.Automation]::Compare(
            [int[]]$provider.element.runtimeId, [int[]]$endpoint.GetRuntimeId())) { return $null }
    $ancestor = $target
    for ($depth = 0; $null -ne $ancestor -and $depth -lt 24; $depth++) {
        if (Same-Element $ancestor $endpoint) {
            return @{method='WindowFromPoint.exactInspectedProvider'; accepted=$true;
                nativeWindow=$nativeHit; endpointRuntimeId=@($endpoint.GetRuntimeId())}
        }
        $ancestor = $walker.GetParent($ancestor)
    }
    return $null
}
try {
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $bodyCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty, [string]$request.body)
    $deadline = [DateTime]::UtcNow.AddSeconds(4)
    $toastMatches = @()
    $discovery = @{}
    $pointDiscovery = @{}
    $nativeWindows = @{}
    $nativeHitProviders = @{}
    $polls = 0
    do {
        $polls++
        $toastMatches = @()
        # Inspect only OS-shell endpoints, never all desktop descendants.
        $endpoints = @{}
        foreach ($element in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $all)) {
            $endpoints[($element.GetRuntimeId() -join ',')] = $element
        }
        # Shell-owned toast HWNDs can be absent from RootElement's children.
        # Resolve only visible windows owned by the same strictly trusted hosts.
        foreach ($hwnd in [ToastProbeNative]::VisibleWindows()) {
            $nativeWindow = Describe-NativeWindow $hwnd
            $nativeWindows[$hwnd.ToInt64()] = $nativeWindow
            if (-not $nativeWindow.trusted) { continue }
            $element = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
            if ($null -ne $element) { $endpoints[($element.GetRuntimeId() -join ',')] = $element }
        }
        # Read-only probes inside the banner rectangle inspected in prior PNGs.
        # A point supplies discovery evidence only, never an action or identity.
        $rect = $request.observedBannerRect
        if ($null -ne $rect -and $rect.Count -eq 4 -and $rect[2] -eq 545 -and $rect[3] -eq 190 -and
            $rect[0] -ge 0 -and $rect[1] -ge 0 -and
            $rect[0] -eq ([ToastProbeNative]::GetSystemMetrics(0) - 570) -and
            $rect[1] -eq ([ToastProbeNative]::GetSystemMetrics(1) - 210)) {
            foreach ($offset in @(@(45,120), @(290,150))) {
                $probe = [System.Windows.Point]::new($rect[0] + $offset[0], $rect[1] + $offset[1])
                $nativeHit = Describe-NativeWindow ([ToastProbeNative]::WindowAt([int]$probe.X, [int]$probe.Y))
                if ($nativeHit.trusted -and $nativeHit.hwnd -ne 0) {
                    # This exact signed-host HWND was hit in the observed toast
                    # rectangle. UWP windows need not appear in EnumWindows or
                    # UIA's desktop children; resolve their provider directly.
                    $providerRecord = @{hwnd=$nativeHit.hwnd; nativeProcessId=$nativeHit.processId;
                        resolved=$false; admitted=$false}
                    try {
                        $nativeElement = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$nativeHit.hwnd)
                        if ($null -ne $nativeElement) {
                            $providerRecord.resolved = $true
                            $sameOwner = $nativeElement.Current.ProcessId -eq $nativeHit.processId
                            $providerRecord.sameOwner = $sameOwner
                            $providerRecord.element = Describe $nativeElement $sameOwner
                            if ($sameOwner) {
                                $endpoints[($nativeElement.GetRuntimeId() -join ',')] = $nativeElement
                                $providerRecord.admitted = $true
                            }
                        }
                    } catch {
                        # Keep errors structural: provider messages may contain
                        # unrelated UI text. Failure never broadens admission.
                        $providerRecord.exceptionType = $_.Exception.GetType().FullName
                        $providerRecord.hresult = $_.Exception.HResult
                    }
                    $nativeHitProviders[$nativeHit.hwnd] = $providerRecord
                }
                $element = [System.Windows.Automation.AutomationElement]::FromPoint($probe)
                $chain = @()
                $outerTrusted = $null
                for ($depth = 0; $null -ne $element -and $depth -lt 24; $depth++) {
                    if (Same-Element $element $root) { break }
                    $accepted = Trusted-Endpoint $element.Current.ProcessId
                    $chain += @{element=(Describe $element $accepted); trust=$endpointTrust[$element.Current.ProcessId]}
                    if ($accepted) { $outerTrusted = $element }
                    $element = $walker.GetParent($element)
                }
                # Retain distinct states, not only the last sample after a
                # banner may have expired. No unrelated UI names are recorded.
                $uiaIds = @($chain | ForEach-Object { $_.element.runtimeId -join ',' }) -join '/'
                $pointKey = ($offset -join ',') + ':' + $nativeHit.hwnd + ':' + $uiaIds
                $sampleMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                if ($pointDiscovery.ContainsKey($pointKey)) {
                    $pointDiscovery[$pointKey].lastObservedTimestampMs = $sampleMs
                    $pointDiscovery[$pointKey].samples++
                } elseif ($pointDiscovery.Count -lt 64) {
                    $pointDiscovery[$pointKey] = @{point=@($probe.X,$probe.Y); ancestors=$chain;
                        nativeWindow=$nativeHit; firstObservedTimestampMs=$sampleMs;
                        lastObservedTimestampMs=$sampleMs; samples=1}
                }
                if ($null -ne $outerTrusted) {
                    $endpoints[($outerTrusted.GetRuntimeId() -join ',')] = $outerTrusted
                }
            }
        }
        $seenBodies = @{}
        foreach ($endpoint in $endpoints.Values) {
            $endpointPid = $endpoint.Current.ProcessId
            $accepted = Trusted-Endpoint $endpointPid
            $key = ($endpoint.GetRuntimeId() -join ',')
            $record = @{processId=$endpointPid; className=$endpoint.Current.ClassName;
                controlType=$endpoint.Current.ControlType.ProgrammaticName;
                trust=$endpointTrust[$endpointPid]; searched=$accepted}
            $discovery[$key] = $record
            if (-not $accepted) { continue }
            $bodies = $endpoint.FindAll($scope, $bodyCondition)
            $record.exactBodyCount = $bodies.Count
            $record.bodyMatches = @()
            foreach ($body in $bodies) {
                $record.bodyMatches += Describe $body
                $bodyKey = $body.GetRuntimeId() -join ','
                if ((Visible-Rect $body) -and -not $seenBodies.ContainsKey($bodyKey)) {
                    $seenBodies[$bodyKey] = $true
                    $toastMatches += ,@($endpoint, $body)
                }
            }
        }
        if ($toastMatches.Count -gt 1) { throw 'Ambiguous fixture marker; no action taken' }
        if ($toastMatches.Count -eq 1) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    @{stage='discovery'; nonce=$request.nonce; polls=$polls;
        observedTimestampMs=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();
        endpoints=@($discovery.Values); pointProbes=@($pointDiscovery.Values);
        nativeWindows=@($nativeWindows.Values);
        nativeHitProviders=@($nativeHitProviders.Values);
        observedBannerRect=$request.observedBannerRect; visibleExactBodyMatches=$toastMatches.Count} |
        ConvertTo-Json -Compress -Depth 12
    if ($toastMatches.Count -ne 1) { throw 'No unique visible fixture toast in a trusted OS endpoint' }
    $endpoint, $body = $toastMatches[0]
    $candidate = $null
    $ancestors = @()
    $element = $body
    for ($depth = 0; $null -ne $element -and $depth -lt 24; $depth++) {
        $ancestors += Describe $element
        $pattern = $null
        if ((Visible-Rect $element) -and
            $element.Current.ProcessId -eq $endpoint.Current.ProcessId -and
            (Has-ExactName $element $request.body) -and
            (Has-ExactName $element $request.title) -and
            (Has-ExactName $element $request.appName) -and
            $element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
            $candidate = $element
            break
        }
        if (Same-Element $element $endpoint) { break }
        $element = $walker.GetParent($element)
    }
    # Emit inspection before the one side effect, even if Invoke blocks/fails.
    $inspection = @{stage='inspection'; nonce=$request.nonce; endpoint=(Describe $endpoint);
        marker=(Describe $body); ancestors=$ancestors; invokeAvailable=($null -ne $candidate)}
    if ($null -ne $candidate) {
        $inspection.target = Describe $candidate
        $descendants = $candidate.FindAll($scope, $all)
        if ($descendants.Count -gt 128) { throw 'Fixture target subtree is too broad; no action taken' }
        $inspection.descendants = @($descendants | ForEach-Object { Describe $_ })
    }
    $inspection | ConvertTo-Json -Compress -Depth 12
    if ($null -eq $candidate) { throw 'Owned toast exposes no safe Invoke ancestor; no click fallback' }
    $point = [System.Windows.Point]::new(0, 0)
    $pointSource = 'provider-clickable-point'
    if (-not $candidate.TryGetClickablePoint([ref]$point)) {
        # A read-only test point in the exact nonce marker, not an input target.
        $markerRect = $body.Current.BoundingRectangle
        $point = [System.Windows.Point]::new($markerRect.X + $markerRect.Width / 2,
                                           $markerRect.Y + $markerRect.Height / 2)
        $pointSource = 'exact-marker-center-read-only'
    }
    $hitTest = Target-UnderPoint $candidate $point $endpoint
    if ($null -eq $hitTest) { throw 'Fixture toast is obscured or not hit-testable' }
    # Revalidate exact content/identity/bounds immediately before invoking once.
    $before = Describe $candidate
    if (-not (Visible-Rect $candidate) -or
        -not (Has-ExactName $candidate $request.body) -or
        -not (Has-ExactName $candidate $request.title) -or
        -not (Has-ExactName $candidate $request.appName) -or
        (($before | ConvertTo-Json -Compress -Depth 5) -cne
         ($inspection.target | ConvertTo-Json -Compress -Depth 5))) {
        throw 'Fixture toast changed before invocation; no action taken'
    }
    $pattern = $candidate.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $actionMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    @{stage='invoking'; nonce=$request.nonce; actionTimestampMs=$actionMs;
        method='UIAutomation.Invoke'; target=$before; point=@($point.X,$point.Y);
        pointSource=$pointSource; hitTest=$hitTest} |
        ConvertTo-Json -Compress -Depth 8
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
    @{stage='invoked'; nonce=$request.nonce; actionTimestampMs=$actionMs;
        returnedTimestampMs=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); count=1} |
        ConvertTo-Json -Compress
} finally {
    [void][ToastProbeNative]::SetThreadDpiAwarenessContext($previousDpi)
}
