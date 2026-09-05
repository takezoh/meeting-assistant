#Requires -Version 5.1

[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [ValidateSet('Debug', 'Release')]
    [string]$Configuration = 'Release',

    [string]$Destination = '',
    [switch]$Build,
    [switch]$SkipUi,
    [switch]$AddToPath
)

. (Join-Path $PSScriptRoot 'common.ps1')

Assert-WindowsHost
$root = Get-RepositoryRoot

if ([string]::IsNullOrWhiteSpace($Destination)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw 'LOCALAPPDATA is unavailable. Supply -Destination explicitly.'
    }
    $Destination = Join-Path $env:LOCALAPPDATA 'MeetingAssistant\dev\bin'
}
$Destination = [IO.Path]::GetFullPath($Destination)

if ($Build) {
    if ($WhatIfPreference) {
        Write-Host "WhatIf: build $Configuration artifacts before installation"
    }
    else {
        & (Join-Path $PSScriptRoot 'build.ps1') -Configuration $Configuration -SkipUi:$SkipUi
        if (-not $?) {
            throw 'Build failed.'
        }
    }
}

$artifacts = Get-BuildArtifacts -Configuration $Configuration -SkipUi:$SkipUi
if (-not $WhatIfPreference) {
    foreach ($artifact in $artifacts) {
        if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
            throw "Build artifact is missing: $artifact. Run build.ps1 first or pass -Build."
        }
    }
}

if ($PSCmdlet.ShouldProcess($Destination, 'Create developer installation directory')) {
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
}

$installedArtifacts = @()
foreach ($artifact in $artifacts) {
    $target = Join-Path $Destination ([IO.Path]::GetFileName($artifact))
    if ($PSCmdlet.ShouldProcess($target, "Copy $artifact")) {
        Copy-Item -LiteralPath $artifact -Destination $target -Force
        $installedArtifacts += [ordered]@{
            name = [IO.Path]::GetFileName($target)
            path = $target
            sha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
}

if (-not $WhatIfPreference) {
    $manifestPath = Join-Path $Destination 'install-manifest.json'
    $manifest = [ordered]@{
        schema_version = 1
        installation_kind = 'developer'
        source_root = $root
        configuration = $Configuration
        installed_at_utc = [DateTime]::UtcNow.ToString('o')
        artifacts = $installedArtifacts
    }
    $json = $manifest | ConvertTo-Json -Depth 5
    if ($PSCmdlet.ShouldProcess($manifestPath, 'Write developer installation manifest')) {
        [IO.File]::WriteAllText($manifestPath, $json, [Text.UTF8Encoding]::new($false))
    }
}

if ($AddToPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $normalizedDestination = $Destination.TrimEnd('\')
    $alreadyPresent = $false
    foreach ($entry in $entries) {
        if ($entry.TrimEnd('\').Equals($normalizedDestination, [StringComparison]::OrdinalIgnoreCase)) {
            $alreadyPresent = $true
            break
        }
    }

    if (-not $alreadyPresent) {
        $newUserPath = (@($entries) + $Destination) -join ';'
        if ($PSCmdlet.ShouldProcess('User PATH', "Append $Destination")) {
            [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
            Write-Host 'User PATH updated. Open a new terminal to use the installed commands.'
        }
    }
}

if ($WhatIfPreference) {
    Write-Host "Developer installation preview complete: $Destination"
}
else {
    Write-Host "Developer installation complete: $Destination"
}
