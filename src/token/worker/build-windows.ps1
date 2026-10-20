param(
    [string]$ToolBin = $env:W64DEVKIT_BIN,
    [string]$X86ToolBin = $env:MACSIGNER_X86_TOOLCHAIN_BIN,
    [switch]$Test
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
if (!$ToolBin) {
    $candidates = @(
        (Join-Path $workspace '.tools/w64devkit/bin'),
        (Join-Path $workspace '../ESIGN/.tools/w64devkit/bin')
    ) | Where-Object { Test-Path (Join-Path $_ 'i686-w64-mingw32-gcc.exe') }
    if (!$candidates) { throw 'Set W64DEVKIT_BIN to a toolchain containing i686-w64-mingw32-gcc.exe.' }
    $ToolBin = @($candidates)[0]
}
$ToolBin = (Resolve-Path $ToolBin).Path
if (!$X86ToolBin) {
    $X86ToolBin = Join-Path $workspace '.tools/ca-research/extracted/winlibs-x86/mingw32/bin'
}
$X86ToolBin = (Resolve-Path $X86ToolBin).Path
$x86Compiler = Join-Path $X86ToolBin 'gcc.exe'
$targetMachine = & $x86Compiler -dumpmachine
if ($LASTEXITCODE -ne 0 -or $targetMachine -ne 'i686-w64-mingw32') {
    throw 'Use a genuine i686 MinGW POSIX/DWARF compiler with the MSVCRT runtime.'
}
$oldPath = $env:PATH
$oldFlags = $env:RUSTFLAGS
$oldLinker = $env:CARGO_TARGET_I686_PC_WINDOWS_GNU_LINKER
$oldHostLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER
$oldWrapper = $env:RUSTC_WRAPPER
try {
    $env:PATH = "$ToolBin;$X86ToolBin;$oldPath"
    $env:RUSTFLAGS = '-C link-self-contained=no'
    $env:CARGO_TARGET_I686_PC_WINDOWS_GNU_LINKER = $x86Compiler
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $ToolBin 'gcc.exe'
    $buildDirectory = Join-Path $workspace '.tools/token-worker-build'
    New-Item -ItemType Directory -Force -Path $buildDirectory | Out-Null
    $wrapper = Join-Path $buildDirectory 'rustc-wrapper.exe'
    & (Join-Path $ToolBin 'gcc.exe') -municode -O2 (Join-Path $PSScriptRoot 'rustc-wrapper.c') -o $wrapper
    if ($LASTEXITCODE -ne 0) { throw 'Cannot build the Rust compiler wrapper.' }
    $env:RUSTC_WRAPPER = $wrapper
    $operation = if ($Test) { 'test' } else { 'build' }
    & "$env:USERPROFILE/.cargo/bin/cargo.exe" '+stable-x86_64-pc-windows-gnu' $operation --locked --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --target i686-pc-windows-gnu --release
    if ($LASTEXITCODE -ne 0) { throw "Worker build failed: $LASTEXITCODE" }
} finally {
    $env:PATH = $oldPath
    $env:RUSTFLAGS = $oldFlags
    $env:CARGO_TARGET_I686_PC_WINDOWS_GNU_LINKER = $oldLinker
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $oldHostLinker
    $env:RUSTC_WRAPPER = $oldWrapper
}
