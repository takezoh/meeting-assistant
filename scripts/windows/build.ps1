#Requires -Version 5.1

[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',

    [switch]$SkipUi,
    [switch]$Verify
)

. (Join-Path $PSScriptRoot 'common.ps1')

Assert-WindowsHost
$root = Get-RepositoryRoot
$cargo = Resolve-Cargo

function New-DeveloperWindowsIcon {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    # tauri-build requires an ICO resource even when bundling is disabled. The
    # production icon is intentionally outside this Phase 1 developer workflow.
    $iconBase64 = 'AAABAAEAAQEAAAEAIAAwAAAAFgAAACgAAAABAAAAAgAAAAEAIAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAKhPX/AAAAAA=='
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    [IO.File]::WriteAllBytes($Path, [Convert]::FromBase64String($iconBase64))
}

if ($Verify) {
    & (Join-Path $PSScriptRoot 'verify.ps1') -SkipUi:$SkipUi
    if (-not $?) {
        throw 'Verification failed.'
    }
}

$profileArguments = @()
if ($Configuration -eq 'Release') {
    $profileArguments += '--release'
}

Push-Location $root
try {
    Invoke-NativeCommand -FilePath $cargo -ArgumentList (@('build', '--locked', '-p', 'ma-engine', '--bins') + $profileArguments)
    Invoke-NativeCommand -FilePath $cargo -ArgumentList (@('build', '--locked', '-p', 'ma-processor-host') + $profileArguments)
    Invoke-NativeCommand -FilePath $cargo -ArgumentList (@('build', '--locked', '-p', 'ma-manifest', '--bin', 'ma-manifest-sign') + $profileArguments)

    if (-not $SkipUi) {
        $developerIcon = Join-Path $root 'target\ui\developer-icon.ico'
        New-DeveloperWindowsIcon -Path $developerIcon

        $hadTauriConfig = Test-Path Env:TAURI_CONFIG
        $previousTauriConfig = $env:TAURI_CONFIG
        try {
            $env:TAURI_CONFIG = [ordered]@{
                bundle = [ordered]@{
                    icon = @($developerIcon)
                }
            } | ConvertTo-Json -Compress

            Invoke-NativeCommand -FilePath $cargo -ArgumentList (@(
                'build',
                '--locked',
                '--manifest-path', 'app/ui/src-tauri/Cargo.toml',
                '--target-dir', (Join-Path $root 'target\ui')
            ) + $profileArguments)
        }
        finally {
            if ($hadTauriConfig) {
                $env:TAURI_CONFIG = $previousTauriConfig
            }
            else {
                Remove-Item Env:TAURI_CONFIG -ErrorAction SilentlyContinue
            }
        }
    }
}
finally {
    Pop-Location
}

$artifacts = Get-BuildArtifacts -Configuration $Configuration -SkipUi:$SkipUi
foreach ($artifact in $artifacts) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
        throw "Expected build artifact was not produced: $artifact"
    }
    Write-Host "Built: $artifact"
}
