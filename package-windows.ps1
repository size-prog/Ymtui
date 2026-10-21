[CmdletBinding()]
param(
    [string]$MpvDirectory,
    [string]$MpvSourceUrl = 'https://github.com/shinchiro/mpv-winbuild-cmake',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $SkipBuild) {
    Push-Location -LiteralPath $PSScriptRoot
    try {
        & cargo build --locked --release
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    } finally { Pop-Location }
}
if (-not $MpvDirectory) {
    foreach ($candidate in @($env:YMTUI_MPV_PATH, 'C:\Program Files\mpv', 'C:\Program Files (x86)\mpv')) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Container) -and (Get-ChildItem -LiteralPath $candidate -Filter '*mpv*.dll')) {
            $MpvDirectory = $candidate
            break
        }
    }
}
if (-not $MpvDirectory) { throw 'Supply -MpvDirectory with libmpv DLLs and their license files.' }
$dlls = @(Get-ChildItem -LiteralPath $MpvDirectory -Filter '*.dll' -File)
if (-not ($dlls | Where-Object Name -in @('libmpv-2.dll', 'mpv-2.dll', 'mpv-1.dll'))) { throw 'libmpv DLL not found.' }
$licenses = @(Get-ChildItem -LiteralPath $MpvDirectory -Filter '*LICENSE*' -File)
if ($licenses.Count -eq 0) { throw 'Include the libmpv license files in -MpvDirectory.' }
$dist = Join-Path $PSScriptRoot 'dist'
$stage = Join-Path $dist ('stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage -Force | Out-Null
try {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\release\ymtui.exe') -Destination $stage
    foreach ($name in @('install.ps1', 'install.cmd', 'uninstall.ps1', 'LICENSE', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $stage
    }
    foreach ($file in $dlls) { Copy-Item -LiteralPath $file.FullName -Destination $stage }
    foreach ($file in $licenses) { Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $stage ('LICENSE-mpv-' + $file.Name)) }
    Set-Content -LiteralPath (Join-Path $stage 'THIRD-PARTY.txt') -Encoding UTF8 -Value @"
ymtui uses libmpv and its bundled dependencies. See LICENSE-mpv-*.
mpv source: https://github.com/mpv-player/mpv
Build scripts and dependency sources: $MpvSourceUrl
Distributors must retain notices and provide corresponding source as required by those licenses.
"@
    $audioCheck = & (Join-Path $stage 'ymtui.exe') --check-audio
    if ($LASTEXITCODE -ne 0) { throw "Packaged audio check failed: $audioCheck" }
    $archive = Join-Path $dist 'ymtui-windows-x64.zip'
    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$archive.sha256" -Encoding ASCII -Value "$hash  ymtui-windows-x64.zip"
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'install-online.ps1') -Destination $dist -Force
    Write-Host "Ready: $archive (includes libmpv; no Rust installation needed)."
} finally {
    $stagePath = [IO.Path]::GetFullPath($stage)
    $distRoot = [IO.Path]::GetFullPath($dist).TrimEnd('\') + '\'
    if ($stagePath.StartsWith($distRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $stagePath -Leaf) -match '^stage-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $stagePath -Recurse -Force
    }
}
