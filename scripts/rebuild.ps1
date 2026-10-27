# One Windows Cargo pipeline. See docs/build-workflow.md for scope and gates.
[CmdletBinding()]
param(
    [ValidateSet('Build', 'Check', 'CheckTests', 'CoreTest', 'DesktopTest')]
    [string]$Task = 'Build',
    [ValidateRange(1, 256)][int]$Jobs = 8,
    [ValidateSet('dev-rebuild', 'dev-fast')][string]$Profile = 'dev-fast',
    [string]$Filter,
    [string]$Example,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$desktopTarget = Join-Path $repo 'target/desktop'
$coreTarget = Join-Path $repo 'crates/super-platinum-core/target'
# User-authorized combined cache budget for the desktop-failure repair phase.
$targetBudgetGiB = 48
$features = 'original-download-native-fixture,protected-native-fixture'
$generatorKey = 'CMAKE_GENERATOR_x86_64_pc_windows_msvc'
$previousPath = $env:PATH
$previousGenerator = [Environment]::GetEnvironmentVariable($generatorKey, 'Process')
$previousStack = $env:RUST_MIN_STACK
$exitCode = 1

if ($Filter -and $Task -notin @('CoreTest', 'DesktopTest')) {
    throw '-Filter is only valid with CoreTest or DesktopTest.'
}
if ($Example -and $Task -ne 'Check') { throw '-Example is only valid with Check.' }
if ($Task -eq 'CoreTest' -and $PSBoundParameters.ContainsKey('Profile')) {
    throw 'CoreTest uses the independently locked core test profile; omit -Profile.'
}

Push-Location $repo
try {
    $overrides = @(Get-ChildItem Env: | Where-Object {
        $_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|CARGO_INCREMENTAL|CARGO_BUILD_INCREMENTAL|CARGO_BUILD_RUSTFLAGS|CARGO_BUILD_TARGET|CARGO_BUILD_BUILD_DIR|CARGO_PROFILE_.+)$'
    })
    if ($overrides) {
        throw ('Remove build/profile environment overrides before using the stable caches: ' + (($overrides | Select-Object -ExpandProperty Name) -join ', '))
    }
    $generator = 'Ninja Multi-Config'
    if ($Task -eq 'CoreTest') {
        # Reuse the surviving core cache without converting its CMake generator.
        $generators = @(Get-ChildItem -Path "$coreTarget/debug/build/btls-sys-*/out/build/CMakeCache.txt" -ErrorAction SilentlyContinue |
            Select-String -Pattern '^CMAKE_GENERATOR:INTERNAL=(.+)$' |
            ForEach-Object { $_.Matches[0].Groups[1].Value } | Select-Object -Unique)
        if ($generators.Count -gt 1) { throw 'Core cache contains conflicting CMake generators; review it without deleting artifacts.' }
        if ($generators.Count -eq 1) { $generator = $generators[0] }
    }
    # Never silently switch CMake generators or create another native cache.
    foreach ($key in @('CMAKE_GENERATOR', 'HOST_CMAKE_GENERATOR',
                       'CMAKE_GENERATOR_x86_64-pc-windows-msvc', $generatorKey)) {
        $value = [Environment]::GetEnvironmentVariable($key, 'Process')
        if ($value -and $value -ne $generator) {
            throw "$key must be $generator for the selected stable Windows cache."
        }
    }
    $ninja = Get-Command ninja.exe -ErrorAction SilentlyContinue
    $ninjaPath = if ($ninja) { $ninja.Source } else { $null }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not $ninjaPath -and (Test-Path -LiteralPath $vswhere)) {
        $ninjaPath = & $vswhere -latest -products '*' -find 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe' |
            Select-Object -First 1
    }
    if ($generator -eq 'Ninja Multi-Config' -and (-not $ninjaPath -or -not (Test-Path -LiteralPath $ninjaPath))) {
        throw 'Ninja is required; install the Visual Studio CMake tools before building.'
    }
    if ($ninjaPath) { $env:PATH = (Split-Path -Parent $ninjaPath) + [IO.Path]::PathSeparator + $previousPath }
    # btls-sys needs the Multi-Config Debug/Release library directories on MSVC.
    [Environment]::SetEnvironmentVariable($generatorKey, $generator, 'Process')
    $env:RUST_MIN_STACK = '16777216'

    $cargoArgs = @(switch ($Task) {
        'Build'       { @('build', '--timings') }
        'Check'       { @('check') }
        'CheckTests'  { @('check', '--tests') }
        'CoreTest'    { @('test', '--manifest-path', 'crates/super-platinum-core/Cargo.toml') }
        'DesktopTest' { @('test') }
    })
    $cargoArgs += @('--locked', '--jobs', "$Jobs")
    if ($Task -eq 'CoreTest') {
        $cargoArgs += @('--target-dir', $coreTarget)
        if ($Filter) { $cargoArgs += '--lib' }
    } else {
        if ($Example) { $cargoArgs += @('--example', $Example) }
        elseif ($Task -ne 'DesktopTest' -or $Filter) { $cargoArgs += @('--bin', 'super-platinum') }
        $cargoArgs += @('--profile', $Profile, '--target-dir', $desktopTarget, '--features', $features)
    }
    if ($Filter) { $cargoArgs += @('--', $Filter) }
    Write-Host ('cargo ' + ($cargoArgs -join ' '))
    Write-Host "CMake: $generator; RUST_MIN_STACK: 16777216; linker/main stack: .cargo/config.toml"
    if ($DryRun) { $exitCode = 0 } else {
        if (Get-Process cargo,rustc -ErrorAction SilentlyContinue) {
            throw 'Another Cargo/rustc pipeline is running. Wait for it to finish.'
        }
        $free = (Get-PSDrive -Name ([IO.Path]::GetPathRoot($repo).Substring(0, 1))).Free
        $targetBytes = 0L
        foreach ($path in @($desktopTarget, $coreTarget)) {
            if (Test-Path -LiteralPath $path) {
                $targetBytes += (Get-ChildItem -LiteralPath $path -Recurse -File -Force |
                    Measure-Object -Property Length -Sum).Sum
            }
        }
        if ($free -lt 50GB -or $targetBytes -ge ($targetBudgetGiB * 1GB)) {
            throw "Storage gate: require 50 GiB free and combined targets below $targetBudgetGiB GiB. Review cleanup separately; nothing was deleted."
        }
        $os = Get-CimInstance Win32_OperatingSystem
        if ($os.FreeVirtualMemory * 1KB -lt 4GB) {
            throw 'Commit safety margin: require at least 4 GiB of system commit headroom before starting.'
        }
        Write-Host ('Available RAM: {0:N2} GiB; commit headroom: {1:N2} GiB; targets: {2:N2} GiB; free disk: {3:N2} GiB.' -f
            ($os.FreePhysicalMemory * 1KB / 1GB), ($os.FreeVirtualMemory * 1KB / 1GB), ($targetBytes / 1GB), ($free / 1GB))
        & cargo @cargoArgs
        $exitCode = $LASTEXITCODE
        if ($exitCode -eq 0 -and $Task -eq 'Build') {
            $binary = Join-Path $desktopTarget "$Profile/super-platinum.exe"
            if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
                throw "Cargo returned success but the executable is missing: $binary"
            }
            Write-Host "Executable: $binary"
        }
    }
} finally {
    $env:PATH = $previousPath
    $env:RUST_MIN_STACK = $previousStack
    [Environment]::SetEnvironmentVariable($generatorKey, $previousGenerator, 'Process')
    Pop-Location
}
exit $exitCode
