#Requires -Version 5.1

[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Position = 0)]
    [ValidateSet('bootstrap', 'verify', 'build', 'install', 'all', 'help')]
    [string]$Action = 'help',

    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',

    [string]$Destination = '',
    [switch]$SkipUi,
    [switch]$SkipWindowsTier,
    [switch]$SkipBuildTools,
    [switch]$SkipRustupInstall,
    [switch]$Build,
    [switch]$AddToPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Show-Usage {
    Write-Host @'
Meeting Assistant Windows developer workflow

Usage:
  .\scripts\windows\dev.ps1 bootstrap [-WhatIf]
  .\scripts\windows\dev.ps1 verify [-SkipUi] [-SkipWindowsTier]
  .\scripts\windows\dev.ps1 build [-Configuration Debug|Release] [-SkipUi]
  .\scripts\windows\dev.ps1 install [-Build] [-Configuration Debug|Release] [-Destination PATH] [-SkipUi] [-AddToPath] [-WhatIf]
  .\scripts\windows\dev.ps1 all [-Configuration Debug|Release] [-Destination PATH] [-SkipUi] [-AddToPath] [-WhatIf]

The install action creates a user-local developer installation. It is not a
signed production installer and does not configure startup behavior.
'@
}

$bootstrapScript = Join-Path $PSScriptRoot 'bootstrap.ps1'
$verifyScript = Join-Path $PSScriptRoot 'verify.ps1'
$buildScript = Join-Path $PSScriptRoot 'build.ps1'
$installScript = Join-Path $PSScriptRoot 'install.ps1'

switch ($Action) {
    'bootstrap' {
        & $bootstrapScript -SkipBuildTools:$SkipBuildTools -SkipRustupInstall:$SkipRustupInstall -WhatIf:$WhatIfPreference
    }
    'verify' {
        & $verifyScript -SkipUi:$SkipUi -SkipWindowsTier:$SkipWindowsTier
    }
    'build' {
        & $buildScript -Configuration $Configuration -SkipUi:$SkipUi
    }
    'install' {
        & $installScript -Build:$Build -Configuration $Configuration -Destination $Destination -SkipUi:$SkipUi -AddToPath:$AddToPath -WhatIf:$WhatIfPreference
    }
    'all' {
        & $bootstrapScript -SkipBuildTools:$SkipBuildTools -SkipRustupInstall:$SkipRustupInstall -WhatIf:$WhatIfPreference
        if ($WhatIfPreference) {
            Write-Host 'WhatIf: verification and build were skipped because bootstrap prerequisites may not exist yet.'
            & $installScript -Configuration $Configuration -Destination $Destination -SkipUi:$SkipUi -AddToPath:$AddToPath -WhatIf
            break
        }
        & $verifyScript -SkipUi:$SkipUi -SkipWindowsTier:$SkipWindowsTier
        & $buildScript -Configuration $Configuration -SkipUi:$SkipUi
        & $installScript -Configuration $Configuration -Destination $Destination -SkipUi:$SkipUi -AddToPath:$AddToPath
    }
    default {
        Show-Usage
    }
}
