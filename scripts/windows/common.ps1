#Requires -Version 5.1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-RepositoryRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
}

function Assert-WindowsHost {
    if ($env:OS -ne 'Windows_NT') {
        throw 'This script must be run from Windows PowerShell or PowerShell on Windows.'
    }
}

function Add-CargoBinToProcessPath {
    if ([string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        return
    }

    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if (-not (Test-Path -LiteralPath $cargoBin -PathType Container)) {
        return
    }

    $pathEntries = @($env:Path -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if (-not ($pathEntries -contains $cargoBin)) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

function Resolve-Executable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [string[]]$FallbackPaths = @()
    )

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    foreach ($candidate in $FallbackPaths) {
        if (-not [string]::IsNullOrWhiteSpace($candidate) -and
            (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    return $null
}

function Resolve-Rustup {
    Add-CargoBinToProcessPath
    $fallback = @()
    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $fallback += (Join-Path $env:USERPROFILE '.cargo\bin\rustup.exe')
    }
    return Resolve-Executable -Name 'rustup.exe' -FallbackPaths $fallback
}

function Resolve-Cargo {
    Add-CargoBinToProcessPath
    $fallback = @()
    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $fallback += (Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe')
    }
    $cargo = Resolve-Executable -Name 'cargo.exe' -FallbackPaths $fallback
    if ($null -eq $cargo) {
        throw 'cargo.exe was not found. Run .\scripts\windows\bootstrap.ps1, then open a new PowerShell window.'
    }
    return $cargo
}

function Invoke-NativeCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$ArgumentList = @(),

        [int[]]$SuccessExitCodes = @(0)
    )

    Write-Host "> $FilePath $($ArgumentList -join ' ')"
    & $FilePath @ArgumentList
    if (-not ($SuccessExitCodes -contains $LASTEXITCODE)) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath"
    }
}

function Get-ProfileName {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('Debug', 'Release')]
        [string]$Configuration
    )

    if ($Configuration -eq 'Release') {
        return 'release'
    }
    return 'debug'
}

function Get-BuildArtifacts {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('Debug', 'Release')]
        [string]$Configuration,

        [switch]$SkipUi
    )

    $root = Get-RepositoryRoot
    $profile = Get-ProfileName -Configuration $Configuration
    $artifacts = @(
        (Join-Path $root "target\$profile\ma-engine.exe"),
        (Join-Path $root "target\$profile\ma-diag.exe"),
        (Join-Path $root "target\$profile\ma-processor-host.exe"),
        (Join-Path $root "target\$profile\ma-manifest-sign.exe")
    )

    if (-not $SkipUi) {
        $artifacts += (Join-Path $root "target\ui\$profile\app-ui.exe")
    }

    return $artifacts
}
