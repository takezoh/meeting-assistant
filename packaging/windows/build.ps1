<#
.SYNOPSIS
    Builds the Meeting Assistant executable that users download and install from.

.DESCRIPTION
    Builds the release executable and copies it to target\windows\Meeting-Assistant-<version>.exe. Opened, that file
    asks whether to install itself. With -Certificate it is also signed and timestamped with signtool.
    docs/technical/packaging.md describes the steps and the signing.

.PARAMETER Certificate
    Subject name of a code-signing certificate in the current user's certificate store to sign with. Without it the
    executable is not signed, and SmartScreen warns whoever downloads it.

.PARAMETER TimestampUrl
    RFC 3161 timestamp server of the certificate's authority. Needed with -Certificate, so that the signature stays
    valid after the certificate expires.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1 -Certificate "<subject>" -TimestampUrl <url>
#>
[CmdletBinding()]
param(
    [string]$Certificate,
    [string]$TimestampUrl
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# PowerShell goes on after a program fails, so each program's exit code is checked.
function Assert-ExitCode([string]$What) {
    if ($LASTEXITCODE -ne 0) {
        throw "$What failed with exit code $LASTEXITCODE."
    }
}

# signtool from the PATH, as in a Developer PowerShell, or else from the newest Windows SDK.
function Find-SignTool {
    $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($onPath) {
        return $onPath.Source
    }
    $architecture = @{ AMD64 = 'x64'; ARM64 = 'arm64'; x86 = 'x86' }[$env:PROCESSOR_ARCHITECTURE]
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $found = Get-ChildItem -Path $kits -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Directory.Name -eq $architecture -and $_.Directory.Parent.Name -match '^\d+(\.\d+)+$' } |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
        Select-Object -First 1
    if (-not $found) {
        throw 'signtool.exe was not found. Install the Windows SDK, or run this script from a Developer PowerShell.'
    }
    return $found.FullName
}

if ($Certificate -and -not $TimestampUrl) {
    throw '-Certificate needs -TimestampUrl, the timestamp server of the certificate''s authority.'
}
if ($TimestampUrl -and -not $Certificate) {
    throw '-TimestampUrl is used only with -Certificate.'
}

# Check what signing needs first, so that nothing is built for a signature that cannot be made.
if ($Certificate) {
    $signTool = Find-SignTool
    $candidates = @(Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Where-Object { $_.Subject -like "*$Certificate*" })
    if ($candidates.Count -eq 0) {
        throw "The current user's certificate store has no code-signing certificate whose subject contains '$Certificate'."
    }
}

$manifest = Join-Path $PSScriptRoot '..\..\Cargo.toml'
$metadata = cargo metadata --format-version 1 --no-deps --manifest-path $manifest | ConvertFrom-Json
Assert-ExitCode 'cargo metadata'
$version = @($metadata.packages | Where-Object { $_.name -eq 'meeting-assistant' })[0].version
$target = $metadata.target_directory

Write-Host "Building Meeting Assistant $version"
cargo build --release --locked -p meeting-assistant --manifest-path $manifest
Assert-ExitCode 'cargo build'

$out = Join-Path $target 'windows'
New-Item -ItemType Directory -Force -Path $out | Out-Null
# The installer copies the file to meeting-assistant.exe, so the download may have any name.
$executable = Join-Path $out "Meeting-Assistant-$version.exe"
Copy-Item -LiteralPath (Join-Path $target 'release\meeting-assistant.exe') -Destination $executable -Force

# The version Explorer shows comes from the resources build.rs embeds; without them it is empty.
$embedded = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
if ($embedded -ne $version) {
    throw "The executable says it is version '$embedded', not $version, so its resources were not embedded."
}

if ($Certificate) {
    Write-Host "Signing with $Certificate"
    & $signTool sign /fd SHA256 /n $Certificate /tr $TimestampUrl /td SHA256 $executable
    Assert-ExitCode 'signtool sign'
    & $signTool verify /pa $executable
    Assert-ExitCode 'signtool verify'
} else {
    Write-Host 'Not signed. SmartScreen warns whoever downloads it; -Certificate names a certificate to sign with.'
}

Write-Output $executable
