[CmdletBinding()]
param(
    [string]$Repository = 'size-prog/Ymtui',
    [string]$Version = 'latest',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\ymtui\bin'),
    [switch]$NoPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'This installer requires 64-bit Windows.'
}
if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw 'Expected repository: owner/name.' }
if ($Version -ne 'latest' -and $Version -notmatch '^v?\d+\.\d+\.\d+([-.][A-Za-z0-9.-]+)?$') { throw 'Invalid release version.' }
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$asset = 'ymtui-windows-x64.zip'
$releasePath = if ($Version -eq 'latest') { 'latest' } else { 'tags/' + $Version }
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/$releasePath" -Headers @{ 'User-Agent' = 'ymtui-installer' }
} catch {
    throw "Cannot download release information. Check your connection and published releases at https://github.com/$Repository/releases . $($_.Exception.Message)"
}
$archiveAsset = @($release.assets | Where-Object name -eq $asset)
$checksumAsset = @($release.assets | Where-Object name -eq "$asset.sha256")
if ($archiveAsset.Count -ne 1 -or $checksumAsset.Count -ne 1) {
    throw "Release has no Windows installer package. Expected $asset and $asset.sha256 in https://github.com/$Repository/releases ."
}
$work = Join-Path ([IO.Path]::GetTempPath()) ('ymtui-install-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $zipPath = Join-Path $work $asset
    $hashPath = Join-Path $work "$asset.sha256"
    Write-Host "Downloading ymtui $($release.tag_name)..."
    Invoke-WebRequest -UseBasicParsing -Uri $archiveAsset[0].browser_download_url -OutFile $zipPath
    Invoke-WebRequest -UseBasicParsing -Uri $checksumAsset[0].browser_download_url -OutFile $hashPath
    $expected = ((Get-Content -LiteralPath $hashPath -Raw).Trim() -split '\s+')[0]
    if ($expected -notmatch '^[a-fA-F0-9]{64}$' -or (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash -ine $expected) {
        throw 'Package SHA256 verification failed. Installation cancelled.'
    }
    $payload = Join-Path $work 'payload'
    Expand-Archive -LiteralPath $zipPath -DestinationPath $payload
    $installerArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $payload 'install.ps1'), '-InstallDir', $InstallDir)
    if ($NoPath) { $installerArgs += '-NoPath' }
    & powershell.exe @installerArgs
    if ($LASTEXITCODE -ne 0) { throw 'Installation failed. Close running ymtui instances and retry.' }
    if (-not $NoPath) {
        # The child updates the persistent user PATH; update this shell too.
        $destination = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
        $entries = @($env:Path -split ';' | Where-Object {
            $_ -and -not [StringComparer]::OrdinalIgnoreCase.Equals(
                [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\'), $destination)
        })
        $env:Path = (@($destination) + $entries) -join ';'
    }
} finally {
    # Only this invocation's GUID-named temporary directory is removed.
    $resolvedWork = [IO.Path]::GetFullPath($work)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($resolvedWork.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedWork -Leaf) -match '^ymtui-install-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $resolvedWork -Recurse -Force
    }
}
