param([string]$Directory = (Join-Path $PSScriptRoot '..\dist\windows-x64'))

$ErrorActionPreference = 'Stop'
$directory = (Resolve-Path -LiteralPath $Directory).Path
$manifest = Join-Path $directory 'identity\AppxManifest.xml'
foreach ($file in @('mue.exe', 'mue_shell.dll', 'ffmpeg\ffmpeg.exe', 'ffmpeg\ffprobe.exe', 'identity\AppxManifest.xml')) {
    if (-not (Test-Path -LiteralPath (Join-Path $directory $file))) { throw "Missing $file. Run scripts/build-windows.ps1 first." }
}
if ([Environment]::OSVersion.Version.Build -lt 22000) { throw 'The modern context menu requires Windows 11.' }
# Loose registration is the local development workflow. Distribution uses a signed identity MSIX.
# Windows rejects same-version developer registration when the manifest has changed.
# Remove only this user's registration; external binaries and Mue/profiles.json are preserved.
Get-AppxPackage -Name 'Mbstdio.Mue' | Remove-AppxPackage
Add-AppxPackage -Register $manifest -ExternalLocation $directory -ForceApplicationShutdown
# Invalidate Explorer's cached associations and context-menu handlers after registration changes.
if (-not ('Mue.ShellNotifications' -as [type])) {
    Add-Type -Namespace Mue -Name ShellNotifications -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("shell32.dll")]
public static extern void SHChangeNotify(uint eventId, uint flags, System.IntPtr item1, System.IntPtr item2);
'@
}
# SHCNE_ASSOCCHANGED with SHCNF_IDLIST | SHCNF_FLUSH waits for the notification to be delivered.
[Mue.ShellNotifications]::SHChangeNotify(0x08000000, 0x1000, [IntPtr]::Zero, [IntPtr]::Zero)
Start-Process -FilePath (Join-Path $directory 'mue.exe') -ArgumentList '--settings'
Write-Host 'Mue registered for the current user. Explorer was notified to refresh its context menu.'
Write-Host 'Close and reopen the context menu. If an old menu remains, restart Windows Explorer or sign out and back in.'
