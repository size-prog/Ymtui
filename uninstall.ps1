[CmdletBinding()]
param([string]$InstallDir = $PSScriptRoot, [switch]$NoPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$destination = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
$markerPath = Join-Path $destination 'install.json'
$marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
if ($marker.app -ne 'ymtui-rust' -or -not [StringComparer]::OrdinalIgnoreCase.Equals($marker.directory, $destination)) {
    throw 'This is not a recognized ymtui installation. No files were removed.'
}
# Remove only installer-owned files. Never delete the directory recursively or user data.
$ownedFiles = @('ymtui.exe', 'uninstall.ps1')
if ($marker.PSObject.Properties['files']) { $ownedFiles += @($marker.files) }
foreach ($name in $ownedFiles | Select-Object -Unique) {
    if ([string]::IsNullOrWhiteSpace($name) -or $name -match '[\\/:]' -or $name -in @('.', '..')) {
        throw 'Invalid file in installation manifest. No further files were removed.'
    }
}
foreach ($name in $ownedFiles | Select-Object -Unique) {
    $ownedPath = Join-Path $destination $name
    if (Test-Path -LiteralPath $ownedPath -PathType Leaf) { Remove-Item -LiteralPath $ownedPath -Force }
}
if (-not $NoPath) {
    foreach ($scope in @('User', 'Process')) {
        $pathValue = [Environment]::GetEnvironmentVariable('Path', $scope)
        $remaining = @($pathValue -split ';' | Where-Object {
            $_ -and -not [StringComparer]::OrdinalIgnoreCase.Equals(
                [Environment]::ExpandEnvironmentVariables($_).TrimEnd('\'), $destination)
        }) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $remaining, $scope)
    }
}
Remove-Item -LiteralPath $markerPath -Force
Write-Host 'ymtui removed. Saved token and settings were preserved. Open a new terminal.'
