$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$sourceRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $sourceRoot ('target\package-smoke-' + [guid]::NewGuid().ToString('N'))
$testDir = Join-Path $testRoot 'installed with spaces'
$zipSource = Join-Path $sourceRoot 'dist\ymtui-windows-x64.zip'
$beforeUser = [Environment]::GetEnvironmentVariable('Path', 'User')
$beforeProcess = $env:Path
$badChecksum = $false
# Exercise the real online installer against the locally built release assets.
function Invoke-RestMethod {
    param($Uri, $Headers)
    return [pscustomobject]@{
        tag_name = 'v-test'
        assets = @(
            [pscustomobject]@{ name = 'ymtui-windows-x64.zip'; browser_download_url = 'https://test.invalid/package' },
            [pscustomobject]@{ name = 'ymtui-windows-x64.zip.sha256'; browser_download_url = 'https://test.invalid/hash' }
        )
    }
}
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, $Uri, $OutFile)
    if ($Uri -eq 'https://test.invalid/package') { Copy-Item -LiteralPath $zipSource -Destination $OutFile }
    elseif ($badChecksum) { Set-Content -LiteralPath $OutFile -Value ('0' * 64) }
    else { Copy-Item -LiteralPath "$zipSource.sha256" -Destination $OutFile }
}
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
    Push-Location -LiteralPath $testRoot
    try {
        & (Join-Path $sourceRoot 'install-online.ps1') -InstallDir $testDir -NoPath
        & (Join-Path $sourceRoot 'install-online.ps1') -InstallDir $testDir -NoPath
    } finally { Pop-Location }
    & (Join-Path $testDir 'ymtui.exe') --version
    if ($LASTEXITCODE -ne 0) { throw 'Installed binary failed.' }
    & (Join-Path $testDir 'ymtui.exe') --check-audio
    if ($LASTEXITCODE -ne 0) { throw 'Bundled libmpv failed.' }
    $badChecksum = $true
    $rejected = $false
    try { & (Join-Path $sourceRoot 'install-online.ps1') -InstallDir $testDir -NoPath }
    catch { if ($_.Exception.Message -match 'SHA256 verification failed') { $rejected = $true } else { throw } }
    if (-not $rejected) { throw 'Invalid checksum was accepted.' }
    $sentinel = Join-Path $testDir 'keep.txt'
    Set-Content -LiteralPath $sentinel -Value 'keep'
    & (Join-Path $testDir 'uninstall.ps1') -NoPath
    if (-not (Test-Path -LiteralPath $sentinel)) { throw 'Uninstall removed unrelated file.' }
    if (Get-ChildItem -LiteralPath $testDir -Filter '*.dll') { throw 'Uninstall left bundled DLL behind.' }
    if ($beforeUser -cne [Environment]::GetEnvironmentVariable('Path', 'User') -or $beforeProcess -cne $env:Path) { throw 'NoPath changed PATH.' }
    Write-Host 'PASS: download, checksum rejection, install from another directory, reinstall, bundled audio, uninstall and NoPath.'
} finally {
    $resolvedTest = [IO.Path]::GetFullPath($testRoot)
    $targetRoot = [IO.Path]::GetFullPath((Join-Path $sourceRoot 'target')).TrimEnd('\') + '\'
    if ($resolvedTest.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $resolvedTest -Leaf) -match '^package-smoke-[a-f0-9]{32}$') {
        Remove-Item -LiteralPath $resolvedTest -Recurse -Force
    }
}
