param([string]$Directory = (Join-Path $PSScriptRoot '..\dist\windows-x64'))

$ErrorActionPreference = 'Stop'
$directory = (Resolve-Path -LiteralPath $Directory).Path
$manifest = Join-Path $directory 'identity\AppxManifest.xml'
foreach ($file in @('mue.exe', 'mue_shell.dll', 'ffmpeg\ffmpeg.exe', 'ffmpeg\ffprobe.exe', 'identity\AppxManifest.xml')) {
    if (-not (Test-Path -LiteralPath (Join-Path $directory $file))) { throw "Missing $file. Run scripts/build-windows.ps1 first." }
}
if ([Environment]::OSVersion.Version.Build -lt 22000) { throw 'The modern context menu requires Windows 11.' }
# Loose registration is the local development workflow. Distribution uses a signed identity MSIX.
Add-AppxPackage -Register $manifest -ExternalLocation $directory -ForceApplicationShutdown
Start-Process -FilePath (Join-Path $directory 'mue.exe') -ArgumentList '--settings'
Write-Host 'Mue registered for the current user. Reopen File Explorer to refresh the context menu.'
