#Requires -Version 5.1

[CmdletBinding()]
param(
    [switch]$SkipUi,
    [switch]$SkipWindowsTier
)

. (Join-Path $PSScriptRoot 'common.ps1')

Assert-WindowsHost
$cargo = Resolve-Cargo
$root = Get-RepositoryRoot

Push-Location $root
try {
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('fmt', '--all', '--', '--check')
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('xtask', 'boundary')
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('xtask', 'verify', '--check-registration')
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('xtask', 'verify', '--tier', 'portable', '--strict')
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('test', '--workspace')
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')

    if (-not $SkipUi) {
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @(
            'test', '--manifest-path', 'app/ui/src-tauri/Cargo.toml', '--no-default-features', '--locked'
        )
    }

    if (-not $SkipWindowsTier) {
        Invoke-NativeCommand -FilePath $cargo -ArgumentList @('xtask', 'verify', '--tier', 'windows', '--strict')
    }
}
finally {
    Pop-Location
}

Write-Host 'Verification complete.'
