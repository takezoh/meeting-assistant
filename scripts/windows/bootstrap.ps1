#Requires -Version 5.1

[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [switch]$SkipBuildTools,
    [switch]$SkipRustupInstall
)

. (Join-Path $PSScriptRoot 'common.ps1')

Assert-WindowsHost

function Get-VsWherePath {
    $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
    if ([string]::IsNullOrWhiteSpace($programFilesX86)) {
        return $null
    }

    $candidate = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        return $candidate
    }
    return $null
}

function Test-MsvcBuildTools {
    $vswhere = Get-VsWherePath
    if ($null -eq $vswhere) {
        return $false
    }

    $installationPath = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace(($installationPath -join '')))
}

function Get-VisualStudioInstallationPath {
    $vswhere = Get-VsWherePath
    if ($null -eq $vswhere) {
        return $null
    }

    $installationPath = & $vswhere -latest -products '*' -property installationPath
    if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace(($installationPath -join ''))) {
        return ($installationPath | Select-Object -First 1)
    }
    return $null
}

if (-not (Test-MsvcBuildTools)) {
    if ($SkipBuildTools) {
        Write-Warning 'Visual Studio C++ x64 build tools were not detected and installation was skipped.'
    }
    else {
        $installationPath = Get-VisualStudioInstallationPath
        $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
        $setup = $null
        if (-not [string]::IsNullOrWhiteSpace($programFilesX86)) {
            $setup = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer\setup.exe'
        }

        if ($null -ne $installationPath -and
            $null -ne $setup -and
            (Test-Path -LiteralPath $setup -PathType Leaf)) {
            if ($PSCmdlet.ShouldProcess($installationPath, 'Add the Visual Studio C++ x64 workload')) {
                Invoke-NativeCommand -FilePath $setup -ArgumentList @(
                    'modify',
                    '--installPath', $installationPath,
                    '--passive', '--norestart',
                    '--add', 'Microsoft.VisualStudio.Workload.VCTools',
                    '--includeRecommended'
                ) -SuccessExitCodes @(0, 3010)
            }
        }
        else {
            $winget = Resolve-Executable -Name 'winget.exe'
            if ($null -eq $winget) {
                throw 'Visual Studio C++ build tools are missing and winget.exe is unavailable. Install Visual Studio 2022 Build Tools with the Desktop development with C++ workload.'
            }

            if ($PSCmdlet.ShouldProcess('Visual Studio 2022 Build Tools', 'Install the C++ x64 workload with winget')) {
                Invoke-NativeCommand -FilePath $winget -ArgumentList @(
                    'install',
                    '--id', 'Microsoft.VisualStudio.2022.BuildTools',
                    '--exact',
                    '--source', 'winget',
                    '--accept-package-agreements',
                    '--accept-source-agreements',
                    '--override', '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
                ) -SuccessExitCodes @(0, 3010)
            }
        }
    }
}
else {
    Write-Host 'Visual Studio C++ x64 build tools: found'
}

if (-not $SkipBuildTools -and -not $WhatIfPreference -and -not (Test-MsvcBuildTools)) {
    throw 'The Visual Studio installer completed, but the C++ x64 workload was not detected. Re-run bootstrap after any pending Visual Studio Installer operation finishes.'
}

$rustup = Resolve-Rustup
if ($null -eq $rustup) {
    if ($SkipRustupInstall) {
        throw 'rustup.exe was not found and installation was skipped.'
    }

    $rustupInstaller = Join-Path $env:TEMP 'rustup-init.exe'
    if ($PSCmdlet.ShouldProcess($rustupInstaller, 'Download and run the official rustup installer')) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri 'https://win.rustup.rs/x86_64' -OutFile $rustupInstaller
            Invoke-NativeCommand -FilePath $rustupInstaller -ArgumentList @(
                '-y', '--default-toolchain', 'stable', '--profile', 'default'
            )
        }
        finally {
            if (Test-Path -LiteralPath $rustupInstaller -PathType Leaf) {
                Remove-Item -LiteralPath $rustupInstaller -Force
            }
        }
        $rustup = Resolve-Rustup
    }
}

if ($null -ne $rustup) {
    if ($PSCmdlet.ShouldProcess('Rust stable toolchain', 'Install/update toolchain and required components')) {
        Invoke-NativeCommand -FilePath $rustup -ArgumentList @('toolchain', 'install', 'stable', '--profile', 'default')
        Invoke-NativeCommand -FilePath $rustup -ArgumentList @('default', 'stable')
        Invoke-NativeCommand -FilePath $rustup -ArgumentList @('component', 'add', 'rustfmt', 'clippy', '--toolchain', 'stable')
    }
}
elseif (-not $WhatIfPreference) {
    throw 'rustup installation completed, but rustup.exe was not found under %USERPROFILE%\.cargo\bin.'
}

if (-not $WhatIfPreference) {
    $cargo = Resolve-Cargo
    Invoke-NativeCommand -FilePath $cargo -ArgumentList @('--version')
}

Write-Host ''
Write-Host 'Bootstrap complete. The current PowerShell process is ready to build.'
