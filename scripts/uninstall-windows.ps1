$ErrorActionPreference = 'Stop'
Get-AppxPackage -Name 'Mbstdio.Mue' | Remove-AppxPackage
Write-Host 'Mue context-menu registration removed. Conversion outputs and user profiles are preserved.'
