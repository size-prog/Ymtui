[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\ymtui\bin'),
    [switch]$SkipBuild,
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Run from any directory. Only the user's PATH is changed; no administrator needed.
$sourceRoot = $PSScriptRoot
$sourceExe = Join-Path $sourceRoot 'ymtui.exe'
$bundled = Test-Path -LiteralPath $sourceExe -PathType Leaf
if (-not $bundled -and -not $SkipBuild) {
    Push-Location -LiteralPath $sourceRoot
    try {
        & cargo build --locked --release
        if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }
    } finally { Pop-Location }
}
if (-not $bundled) { $sourceExe = Join-Path $sourceRoot 'target\release\ymtui.exe' }
if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw 'Release executable not found. Run install.ps1 without -SkipBuild.'
}
$version = & $sourceExe --version
if ($LASTEXITCODE -ne 0 -or $version -notmatch '^ymtui \d+\.\d+\.\d+') {
    throw 'Release executable validation failed.'
}
$destination = [System.IO.Path]::GetFullPath($InstallDir)
if ($destination.Contains(';')) { throw 'Install directory cannot contain a semicolon.' }
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$targetExe = Join-Path $destination 'ymtui.exe'
if (-not [StringComparer]::OrdinalIgnoreCase.Equals($sourceExe, $targetExe)) {
    try { Copy-Item -LiteralPath $sourceExe -Destination $targetExe -Force }
    catch { throw "Cannot update $targetExe. Close running ymtui instances and retry. $($_.Exception.Message)" }
}
Copy-Item -LiteralPath (Join-Path $sourceRoot 'uninstall.ps1') -Destination (Join-Path $destination 'uninstall.ps1') -Force
$ownedFiles = @('ymtui.exe', 'uninstall.ps1')
if ($bundled) {
    foreach ($file in Get-ChildItem -LiteralPath $sourceRoot -File | Where-Object { $_.Extension -eq '.dll' -or $_.Name -like 'LICENSE*' -or $_.Name -eq 'THIRD-PARTY.txt' }) {
        if (-not [StringComparer]::OrdinalIgnoreCase.Equals($sourceRoot.TrimEnd('\'), $destination.TrimEnd('\'))) {
            Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $destination $file.Name) -Force
        }
        $ownedFiles += $file.Name
    }
}
# Keep ownership of dependencies when updating a bundled install from sources.
$oldMarker = Join-Path $destination 'install.json'
if (Test-Path -LiteralPath $oldMarker) {
    $old = Get-Content -LiteralPath $oldMarker -Raw | ConvertFrom-Json
    if ($old.app -eq 'ymtui-rust' -and $old.PSObject.Properties['files']) {
        $ownedFiles += @($old.files)
    }
}
$marker = @{ app = 'ymtui-rust'; version = [string]$version; directory = $destination; files = @($ownedFiles | Select-Object -Unique) } | ConvertTo-Json
Set-Content -LiteralPath (Join-Path $destination 'install.json') -Value $marker -Encoding UTF8

function Add-FirstPathEntry([string]$Value, [string]$Entry) {
    $entries = @($Value -split ';' | Where-Object {
        $_ -and -not [StringComparer]::OrdinalIgnoreCase.Equals(
            [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\'), $Entry.TrimEnd('\'))
    })
    return (@($Entry) + $entries) -join ';'
}
if (-not $NoPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $updatedPath = Add-FirstPathEntry $userPath $destination
    if ($updatedPath -cne $userPath) {
        [Environment]::SetEnvironmentVariable('Path', $updatedPath, 'User')
    }
    $env:Path = Add-FirstPathEntry $env:Path $destination
    # Let Explorer and terminals discover the updated user environment.
    if (-not ('Ymtui.EnvironmentBroadcast' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Ymtui {
    public static class EnvironmentBroadcast {
        [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam,
            string lParam, uint flags, uint timeout, out UIntPtr result);
    }
}
'@
    }
    $broadcastResult = [UIntPtr]::Zero
    [void][Ymtui.EnvironmentBroadcast]::SendMessageTimeout([IntPtr]0xffff, 0x001A,
        [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$broadcastResult)
    $resolved = (Get-Command ymtui -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
    if (-not [StringComparer]::OrdinalIgnoreCase.Equals($resolved, $targetExe)) {
        Write-Warning "Another command takes precedence: $resolved. Installed executable: $targetExe"
    }
}
Write-Host "Installed $version at $targetExe"
Write-Host 'Start: ymtui   |   Demo: ymtui --demo   |   Sign in: ymtui --login'
if (-not $NoPath) { Write-Host 'Open a new terminal to use the updated PATH in other windows.' }
