$ErrorActionPreference = 'Stop'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$testDir = Join-Path $sourceRoot 'target\install-smoke'
$beforeUser = [Environment]::GetEnvironmentVariable('Path', 'User')
$beforeProcess = $env:Path
& (Join-Path $sourceRoot 'install.ps1') -InstallDir $testDir -SkipBuild -NoPath
& (Join-Path $sourceRoot 'install.ps1') -InstallDir $testDir -SkipBuild -NoPath
$output = & (Join-Path $testDir 'ymtui.exe') --version
if ($LASTEXITCODE -ne 0 -or $output -notmatch '^ymtui 1\.0\.0$') { throw 'Installed executable validation failed.' }
# Unknown files must survive uninstall.
$sentinel = Join-Path $testDir 'keep.txt'
Set-Content -LiteralPath $sentinel -Value 'keep'
& (Join-Path $sourceRoot 'uninstall.ps1') -InstallDir $testDir -NoPath
if (Test-Path -LiteralPath (Join-Path $testDir 'ymtui.exe')) { throw 'Uninstall left executable behind.' }
if (-not (Test-Path -LiteralPath $sentinel)) { throw 'Uninstall removed an unrelated file.' }
if ($beforeUser -cne [Environment]::GetEnvironmentVariable('Path', 'User') -or $beforeProcess -cne $env:Path) { throw 'NoPath changed PATH.' }
Remove-Item -LiteralPath $sentinel
Write-Host 'PASS: install, reinstall, version, uninstall, unrelated files and NoPath.'
