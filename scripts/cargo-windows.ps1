# Runs cargo with the GNU toolchain and a portable w64devkit linker, so no Visual
# Studio installation or system PATH change is needed.
# Usage: scripts/cargo-windows.ps1 test --locked
#        scripts/cargo-windows.ps1 run --release
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$candidates = @(
    $env:W64DEVKIT_BIN,
    (Join-Path $root '.tools\w64devkit\bin'),
    (Join-Path $root '..\CTSIGN\.tools\w64devkit\bin'),
    (Join-Path $root '..\ESIGN\.tools\w64devkit\bin')
) | Where-Object { $_ -and (Test-Path (Join-Path $_ 'gcc.exe')) }
if (!$candidates) { throw 'Install portable w64devkit under .tools/w64devkit or set W64DEVKIT_BIN.' }
$toolBin = (Resolve-Path @($candidates)[0]).Path
$env:PATH = "$toolBin;$env:PATH"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $toolBin 'gcc.exe'
$env:RUSTFLAGS = '-C link-self-contained=yes'
$ErrorActionPreference = 'Continue'
Push-Location $root
try {
    & $cargo '+stable-x86_64-pc-windows-gnu' @args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }
