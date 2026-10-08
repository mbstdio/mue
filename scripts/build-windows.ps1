param([switch]$Debug)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$profile = if ($Debug) { 'debug' } else { 'release' }
$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$mt = Get-ChildItem -LiteralPath $sdk -Filter 'mt.exe' -Recurse |
    Where-Object { $_.Directory.Name -eq 'x64' } | Sort-Object FullName -Descending |
    Select-Object -First 1
if (-not $mt) { throw 'Install the Windows SDK (Manifest Tool / mt.exe) before packaging Mue.' }

$arguments = @('build', '--workspace', '--locked', '--manifest-path', (Join-Path $root 'Cargo.toml'))
if (-not $Debug) { $arguments += '--release' }
& cargo @arguments
if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed.' }

$destination = Join-Path $root 'dist\windows-x64'
$identity = Join-Path $destination 'identity'
New-Item -ItemType Directory -Path $identity -Force | Out-Null
foreach ($binary in @('mue.exe', 'mue_shell.dll')) {
    Copy-Item -LiteralPath (Join-Path $root "target\$profile\$binary") -Destination $destination -Force
}
$executable = Join-Path $destination 'mue.exe'
& $mt.FullName -nologo -manifest (Join-Path $root 'packaging\windows\mue.manifest') "-outputresource:$executable;#1"
if ($LASTEXITCODE -ne 0) { throw 'Cannot embed the application identity manifest.' }

Add-Type -AssemblyName System.Drawing
$assets = Join-Path $destination 'Assets'
New-Item -ItemType Directory -Path $assets -Force | Out-Null
foreach ($asset in @(@('StoreLogo.png', 50), @('Square44x44Logo.png', 44), @('Square150x150Logo.png', 150), @('Mue.png', 32))) {
    $dimension = [int]$asset[1]
    $bitmap = [System.Drawing.Bitmap]::new($dimension, $dimension)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([System.Drawing.Color]::FromArgb(88, 65, 207))
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $pen = [System.Drawing.Pen]::new([System.Drawing.Color]::White, ($dimension * 0.1))
    $points = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new(($dimension * 0.25), ($dimension * 0.78)),
        [System.Drawing.PointF]::new(($dimension * 0.25), ($dimension * 0.25)),
        [System.Drawing.PointF]::new(($dimension * 0.50), ($dimension * 0.52)),
        [System.Drawing.PointF]::new(($dimension * 0.75), ($dimension * 0.25)),
        [System.Drawing.PointF]::new(($dimension * 0.75), ($dimension * 0.78))
    )
    $graphics.DrawLines($pen, $points)
    $bitmap.Save((Join-Path $assets $asset[0]), [System.Drawing.Imaging.ImageFormat]::Png)
    $pen.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
}
# An ICO can contain a PNG image directly; avoid native icon handles and their ownership rules.
$png = [System.IO.File]::ReadAllBytes((Join-Path $assets 'Mue.png'))
$stream = [System.IO.File]::Create((Join-Path $assets 'Mue.ico'))
$writer = New-Object System.IO.BinaryWriter($stream)
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]1)
    $writer.Write([byte]32); $writer.Write([byte]32); $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32]$png.Length); $writer.Write([uint32]22)
    $writer.Write($png)
} finally { $writer.Dispose() }
Copy-Item -LiteralPath $assets -Destination $identity -Recurse -Force
Copy-Item -LiteralPath (Join-Path $root 'packaging\windows\AppxManifest.xml') -Destination $identity -Force
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $destination -Force
& (Join-Path $PSScriptRoot 'bundle-ffmpeg.ps1') -Destination (Join-Path $destination 'ffmpeg')
Write-Host "Mue is ready in $destination"
Write-Host 'Register the Windows 11 context menu with scripts/install-windows.ps1.'
