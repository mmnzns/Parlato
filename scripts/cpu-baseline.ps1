# Parlato: Windows builds compile whisper.cpp and llama.cpp (ggml) for a fixed CPU baseline, never for the machine
# that builds them. whisper.cpp defaults to GGML_NATIVE=ON, which compiles for whatever CPU runs the build: a GitHub
# runner with AVX-512 (or a Zen 4/5 PC) would ship a parla.exe that stops with "illegal instruction" on most Intel
# desktops and laptops the first time Whisper runs. On ARM64 the same default means -mcpu=native for the runner's
# server CPU, whose SVE/i8mm a Snapdragon laptop may not have.
#
# x64 baseline: x86-64-v3 (SSE4.2, AVX, AVX2, FMA, F16C, BMI2; every PC since about 2015), no AVX-512, no AVX-VNNI.
# ARM64 baseline: plain armv8-a, the same as llama.cpp already gets.
#
# whisper-rs-sys forwards every GGML_* environment variable as a CMake define. llama-cpp-sys-2 doesn't, but it
# already forces GGML_NATIVE=OFF itself, and ggml then turns AVX/AVX2/FMA/F16C/BMI2/SSE4.2 on by default. Cargo does
# not rerun the -sys build scripts when only the environment changed, hence Reset-StaleGgml.
#
# Used by .github/workflows/release.yml (and ci.yml) on Windows only: the Mac build is native on Apple silicon and
# stays as it was. For a local release build you will share, dot-source this and call Set-GgmlBaseline first.

$ggmlX64On = 'GGML_SSE42', 'GGML_AVX', 'GGML_AVX2', 'GGML_BMI2', 'GGML_FMA', 'GGML_F16C'
$ggmlX64Off = 'GGML_AVX_VNNI', 'GGML_AVX512', 'GGML_AVX512_VBMI', 'GGML_AVX512_VNNI', 'GGML_AVX512_BF16'

# Sets the baseline in this session and, on GitHub Actions, for the job's later steps.
function Set-GgmlBaseline([ValidateSet('x64', 'arm64')][string]$arch) {
  $vars = [ordered]@{ GGML_NATIVE = 'OFF' }
  if ($arch -eq 'x64') {
    foreach ($on in $ggmlX64On) { $vars[$on] = 'ON' }
    foreach ($off in $ggmlX64Off) { $vars[$off] = 'OFF' }
  }
  foreach ($name in $vars.Keys) {
    Set-Item "env:$name" $vars[$name]
    if ($env:GITHUB_ENV) { "$name=$($vars[$name])" | Out-File $env:GITHUB_ENV -Append -Encoding utf8 }
  }
}

# The CMake caches of whisper.cpp and llama.cpp in a profile folder, e.g. src-tauri/target/<triple>/release.
function Get-GgmlCaches([string]$profileDir) {
  $build = Join-Path $profileDir 'build'
  if (-not (Test-Path $build)) { return @() }
  Get-ChildItem $build -Directory |
    Where-Object { $_.Name -match '^(whisper-rs-sys|llama-cpp-sys-2)-' } |
    ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
}

# What is wrong with a cache: built for the build machine's CPU, or (x64) with AVX-512 / AVX-VNNI, or without AVX2.
function Test-GgmlBaseline([string]$cache, [string]$arch) {
  $text = Get-Content $cache
  $bad = @($text | Where-Object { $_ -match '^GGML_NATIVE:BOOL=ON' })
  if ($arch -eq 'x64') {
    $bad += @($text | Where-Object { $_ -match '^GGML_(AVX512[A-Z0-9_]*|AVX_VNNI):BOOL=ON' })
    if (-not ($text | Where-Object { $_ -match '^GGML_AVX2:BOOL=ON' })) { $bad += 'GGML_AVX2 is not ON' }
  }
  $bad
}

# Cleans the two -sys crates when a cache from an earlier build (or the Rust cache) doesn't match the baseline.
# $cargoProfile is 'release' or 'dev'; $triple is empty for a build without --target.
function Reset-StaleGgml([string]$manifest, [string]$arch, [string]$cargoProfile, [string]$triple) {
  $stale = @(Get-GgmlCaches (Get-GgmlProfileDir $manifest $cargoProfile $triple) |
    Where-Object { @(Test-GgmlBaseline $_ $arch).Count -gt 0 })
  if ($stale.Count -eq 0) { return }
  Write-Host "Rebuilding whisper.cpp and llama.cpp for the $arch baseline"
  $cleanArgs = @('clean', '--manifest-path', $manifest, '--profile', $cargoProfile, '-p', 'whisper-rs-sys', '-p', 'llama-cpp-sys-2')
  if ($triple) { $cleanArgs += @('--target', $triple) }
  cargo @cleanArgs
  if ($LASTEXITCODE -ne 0) { throw "cargo clean failed ($LASTEXITCODE)" }
}

# Fails unless both caches exist and match the baseline.
function Assert-GgmlBaseline([string]$manifest, [string]$arch, [string]$cargoProfile, [string]$triple) {
  $caches = @(Get-GgmlCaches (Get-GgmlProfileDir $manifest $cargoProfile $triple))
  if ($caches.Count -lt 2) { throw "Expected the whisper.cpp and llama.cpp CMake caches, found $($caches.Count)" }
  foreach ($c in $caches) {
    $bad = @(Test-GgmlBaseline $c $arch)
    if ($bad.Count -gt 0) { throw "Not built for the $arch baseline: $c ($($bad -join '; '))" }
    Write-Host "OK: $c"
    Get-Content $c | Where-Object { $_ -match '^GGML_(NATIVE|AVX[A-Z0-9_]*|FMA|F16C|BMI2|SSE42):BOOL=' } |
      ForEach-Object { Write-Host "  $_" }
  }
}

function Get-GgmlProfileDir([string]$manifest, [string]$cargoProfile, [string]$triple) {
  $dir = Join-Path (Split-Path $manifest -Parent) 'target'
  if ($triple) { $dir = Join-Path $dir $triple }
  Join-Path $dir $(if ($cargoProfile -eq 'dev') { 'debug' } else { $cargoProfile })
}
