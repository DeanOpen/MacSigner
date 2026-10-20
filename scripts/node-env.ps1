# Puts the portable Node.js in .tools/node (and the GNU Rust linker) on PATH for
# this shell only, then runs the given command. Example:
#   scripts/node-env.ps1 pnpm install
#   scripts/node-env.ps1 pnpm tauri dev
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$node = Join-Path $root '.tools\node'
if (!(Test-Path (Join-Path $node 'node.exe'))) { throw 'Portable Node.js missing: expected .tools/node/node.exe' }
$env:PATH = "$node;$(Join-Path $node 'node_modules\corepack\shims');$env:PATH"
$env:COREPACK_HOME = Join-Path $root '.tools\corepack'
$env:COREPACK_ENABLE_DOWNLOAD_PROMPT = '0'
$candidates = @(
    $env:W64DEVKIT_BIN,
    (Join-Path $root '.tools\w64devkit\bin'),
    (Join-Path $root '..\CTSIGN\.tools\w64devkit\bin'),
    (Join-Path $root '..\ESIGN\.tools\w64devkit\bin')
) | Where-Object { $_ -and (Test-Path (Join-Path $_ 'gcc.exe')) }
if ($candidates) {
    $toolBin = (Resolve-Path @($candidates)[0]).Path
    $env:PATH = "$toolBin;$(Join-Path $env:USERPROFILE '.cargo\bin');$env:PATH"
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $toolBin 'gcc.exe'
    $env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
    $env:RUSTFLAGS = '-C link-self-contained=yes'
}
$ErrorActionPreference = 'Continue'
Push-Location (Join-Path $root 'app')
try {
    $cmd, $rest = $args
    & $cmd @rest
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }
