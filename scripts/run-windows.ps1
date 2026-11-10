# Runs a FerriticUI example on Windows with the environment it needs.
#
#   powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1 -Example counter
#   powershell -ExecutionPolicy Bypass -File scripts\run-windows.ps1 -Example counter --snapshot
#
# Two pieces of environment are handled here:
# * BLADE_DXC_PATH: the D3D12 backend and some shader paths load dxcompiler.dll at
#   runtime; the Windows SDK ships one.
# * VK_DRIVER_FILES: blade's Vulkan HAL on Windows fails with VK_ERROR_INCOMPLETE
#   on machines where the loader also sees an old/broken integrated-GPU Vulkan
#   ICD. Pointing the loader at NVIDIA's nv-vk64.json is enough on such machines.
#
# Run this from a normal (non-elevated) terminal: the Vulkan loader ignores
# VK_DRIVER_FILES in elevated processes.
param(
    [string]$Example = 'counter'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# Newest Windows SDK's DXC, if present.
$kits = 'C:\Program Files (x86)\Windows Kits\10\bin'
if (Test-Path $kits) {
    $sdk = Get-ChildItem $kits -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($sdk) {
        $dxc = Join-Path $sdk.FullName 'x64\dxcompiler.dll'
        if (Test-Path $dxc) {
            if (-not $env:BLADE_DXC_PATH) { $env:BLADE_DXC_PATH = $dxc }
            $env:PATH = (Join-Path $sdk.FullName 'x64') + ';' + $env:PATH
        }
    }
}

# Prefer NVIDIA's Vulkan ICD if the machine has a hybrid/integrated GPU whose
# Vulkan ICD breaks enumeration (VK_ERROR_INCOMPLETE).
if (-not $env:VK_DRIVER_FILES) {
    # Prefer a manifest already registered by the active driver, else the newest
    # nv-vk64.json in the DriverStore (older driver folders can be stale).
    $icdPath = $null
    $registered = Get-ItemProperty 'HKLM:\SOFTWARE\Khronos\Vulkan\Drivers' -ErrorAction SilentlyContinue
    if ($registered) {
        $entry = $registered.PSObject.Properties |
            Where-Object { $_.Name -like '*nv-vk64.json' } | Select-Object -First 1
        if ($entry) { $icdPath = $entry.Name }
    }
    if (-not $icdPath) {
        $newest = Get-ChildItem 'C:\Windows\System32\DriverStore\FileRepository' -Recurse `
            -Filter 'nv-vk64.json' -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($newest) { $icdPath = $newest.FullName }
    }
    if ($icdPath) {
        $env:VK_DRIVER_FILES = $icdPath
        $env:VK_ICD_FILENAMES = $icdPath
    }
}

# Find cargo even when the calling shell (for example a scheduled task) does not
# have the Rust toolchain in PATH.
$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
if (-not $cargo) {
    $toolchain = Join-Path $env:USERPROFILE '.cargo\bin'
    if (Test-Path (Join-Path $toolchain 'cargo.exe')) {
        $env:PATH = $toolchain + ';' + $env:PATH
        $cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue
    }
}
if (-not $cargo) {
    throw 'cargo not found: install Rust or add %USERPROFILE%\.cargo\bin to PATH'
}

Set-Location $root
& cargo run -p ferritic --example $Example -- @args
exit $LASTEXITCODE
