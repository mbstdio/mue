param(
    [string]$Destination = (Join-Path $PSScriptRoot '..\target\debug\ffmpeg')
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$pin = Get-Content -LiteralPath (Join-Path $root 'packaging\ffmpeg.json') -Raw | ConvertFrom-Json
$build = $pin.windows_x64
$cache = Join-Path $root "vendor\ffmpeg\$($pin.version)"
New-Item -ItemType Directory -Path $cache -Force | Out-Null
$archive = Join-Path $cache 'essentials.zip'

if (-not (Test-Path -LiteralPath $archive)) {
    Write-Host "Downloading FFmpeg $($pin.version)..."
    Invoke-WebRequest -Uri $build.url -OutFile "$archive.partial" -UseBasicParsing
    Move-Item -LiteralPath "$archive.partial" -Destination $archive -Force
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $build.sha256) {
    Remove-Item -LiteralPath $archive
    throw 'FFmpeg archive checksum mismatch. The cached archive was removed.'
}
$expanded = Join-Path $cache $build.directory
if (-not (Test-Path -LiteralPath $expanded)) {
    Expand-Archive -LiteralPath $archive -DestinationPath $cache -Force
}
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
foreach ($tool in @('ffmpeg', 'ffprobe')) {
    Copy-Item -LiteralPath (Join-Path $expanded "bin\$tool.exe") -Destination $Destination -Force
    $version = & (Join-Path $Destination "$tool.exe") -version
    if ($LASTEXITCODE -ne 0 -or $version[0] -notmatch "^$tool version $([regex]::Escape($pin.version))(-| )") {
        throw "Bundled $tool does not match the pinned version."
    }
}
Copy-Item -LiteralPath (Join-Path $expanded 'LICENSE') -Destination $Destination -Force
Copy-Item -LiteralPath (Join-Path $expanded 'README.txt') -Destination $Destination -Force
Copy-Item -LiteralPath (Join-Path $root 'packaging\ffmpeg.json') -Destination (Join-Path $Destination 'build.json') -Force
Write-Host "Bundled FFmpeg and ffprobe $($pin.version) in $Destination"
