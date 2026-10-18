# Windows cold-build path: Cargo/LLVM + Ninja native compilation, up to 12 jobs.
[CmdletBinding()]
param(
    [ValidateRange(1, 256)][int]$Jobs = 12,
    [string]$TargetDir,
    [string]$Features
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$previousPath = $env:PATH
$generatorKey = 'CMAKE_GENERATOR_x86_64_pc_windows_msvc'
$previousGenerator = [Environment]::GetEnvironmentVariable($generatorKey, 'Process')
$exitCode = 1

Push-Location $repo
try {
    # Honor explicit CMake choices. Otherwise prefer Ninja from PATH or VS.
    $explicitGenerator = $false
    foreach ($key in @('CMAKE_GENERATOR', 'HOST_CMAKE_GENERATOR',
                       'CMAKE_GENERATOR_x86_64-pc-windows-msvc', $generatorKey)) {
        if ([Environment]::GetEnvironmentVariable($key, 'Process')) {
            $explicitGenerator = $true
        }
    }
    $backend = 'default'
    if (-not $explicitGenerator) {
        $ninja = Get-Command ninja.exe -ErrorAction SilentlyContinue
        $ninjaPath = if ($ninja) { $ninja.Source } else { $null }
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (-not $ninjaPath -and (Test-Path -LiteralPath $vswhere)) {
            $ninjaPath = & $vswhere -latest -products '*' -find 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja/ninja.exe' |
                Select-Object -First 1
        }
        if ($ninjaPath -and (Test-Path -LiteralPath $ninjaPath)) {
            $env:PATH = (Split-Path -Parent $ninjaPath) + [IO.Path]::PathSeparator + $previousPath
            # btls-sys expects MSVC libraries under Debug/Release. Plain Ninja
            # makes it emit a trailing-backslash search path, which breaks
            # quoting in rustc's Windows linker response file for this build.
            [Environment]::SetEnvironmentVariable($generatorKey, 'Ninja Multi-Config', 'Process')
            $backend = 'ninja-multi'
        }
    }
    # CMake cannot switch generators in an existing build directory. Keep the
    # auto-selected Ninja cache separate from the default/custom generator cache.
    if (-not $TargetDir) { $TargetDir = "target/rebuild-$backend" }
    Write-Host "Building dev-rebuild with $Jobs Cargo/native jobs (CMake: $backend)."
    $cargoArgs = @('rebuild', '--jobs', "$Jobs", '--target-dir', $TargetDir)
    if ($Features) { $cargoArgs += @('--features', $Features) }
    & cargo @cargoArgs
    $exitCode = $LASTEXITCODE
    if ($exitCode -eq 0) {
        $binary = Join-Path $TargetDir 'dev-rebuild/super-platinum.exe'
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
            throw "Cargo returned success but the executable is missing: $binary"
        }
        Write-Host "Executable: $binary"
    }
} finally {
    $env:PATH = $previousPath
    [Environment]::SetEnvironmentVariable($generatorKey, $previousGenerator, 'Process')
    Pop-Location
}
exit $exitCode
