# Parlato: Windows builds compile whisper.cpp (ggml, through whisper-rs-sys, in the app's own process) for a fixed CPU
# baseline, never for the machine that builds them. llama.cpp is no longer in the app: it runs in the parlato-llm
# helper, which the engine's reusable workflow builds and checks (its baseline and -O2 check are the engine's). whisper.cpp defaults to GGML_NATIVE=ON, which compiles for whatever CPU runs the build: a GitHub
# runner with AVX-512 (or a Zen 4/5 PC) would ship a parla.exe that stops with "illegal instruction" on most Intel
# desktops and laptops the first time Whisper runs. On ARM64 the same default means -mcpu=native for the runner's
# server CPU, whose SVE/i8mm a Snapdragon laptop may not have.
#
# x64 baseline: x86-64-v3 (SSE4.2, AVX, AVX2, FMA, F16C, BMI2; every PC since about 2015), no AVX-512, no AVX-VNNI.
# ARM64 baseline: plain armv8-a (ggml's own default for the compiler, the same as the engine's helper gets).
#
# whisper-rs-sys forwards every GGML_* environment variable as a CMake define. .cargo/config.toml at the repo root (the engine's
# "Build settings an app must copy") sets the same values and CMAKE_C/CXX_FLAGS_RELEASE=-O2 -DNDEBUG; this script
# repeats the ggml ones for the CI jobs and checks the result. Cargo does not rerun the -sys build script when only
# the environment changed, hence Reset-StaleGgml. For release builds the cache must also carry -O2: cmake-rs strips /O
# from the release flags under the Visual Studio generator, so without the two CMAKE_*_FLAGS_RELEASE values whisper.cpp
# is built at /Od and runs about ten times slower.
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

# The CMake cache of whisper.cpp in a profile folder, e.g. src-tauri/target/<triple>/release.
function Get-GgmlCaches([string]$profileDir) {
  $build = Join-Path $profileDir 'build'
  if (-not (Test-Path $build)) { return @() }
  Get-ChildItem $build -Directory |
    Where-Object { $_.Name -match '^whisper-rs-sys-' } |
    ForEach-Object { Join-Path $_.FullName 'out\build\CMakeCache.txt' } | Where-Object { Test-Path $_ }
}

# What is wrong with a cache: built for the build machine's CPU, or (x64) with AVX-512 / AVX-VNNI, or without AVX2, or
# (release builds) without -O2 in CMAKE_C_FLAGS_RELEASE / CMAKE_CXX_FLAGS_RELEASE.
function Test-GgmlBaseline([string]$cache, [string]$arch, [string]$cargoProfile = 'dev') {
  $text = Get-Content $cache
  $bad = @($text | Where-Object { $_ -match '^GGML_NATIVE:BOOL=ON' })
  if ($cargoProfile -eq 'release') {
    foreach ($v in 'CMAKE_C_FLAGS_RELEASE', 'CMAKE_CXX_FLAGS_RELEASE') {
      if (-not ($text | Where-Object { $_ -match "^${v}:[A-Z]+=.*-O2" })) { $bad += "$v has no -O2" }
    }
  }
  if ($arch -eq 'x64') {
    $bad += @($text | Where-Object { $_ -match '^GGML_(AVX512[A-Z0-9_]*|AVX_VNNI):BOOL=ON' })
    if (-not ($text | Where-Object { $_ -match '^GGML_AVX2:BOOL=ON' })) { $bad += 'GGML_AVX2 is not ON' }
  }
  $bad
}

# Cleans whisper-rs-sys when a cache from an earlier build (or the Rust cache) doesn't match the baseline.
# $cargoProfile is 'release' or 'dev'; $triple is empty for a build without --target.
function Reset-StaleGgml([string]$manifest, [string]$arch, [string]$cargoProfile, [string]$triple) {
  $stale = @(Get-GgmlCaches (Get-GgmlProfileDir $manifest $cargoProfile $triple) |
    Where-Object { @(Test-GgmlBaseline $_ $arch $cargoProfile).Count -gt 0 })
  if ($stale.Count -eq 0) { return }
  Write-Host "Rebuilding whisper.cpp for the $arch baseline"
  $cleanArgs = @('clean', '--manifest-path', $manifest, '--profile', $cargoProfile, '-p', 'whisper-rs-sys')
  if ($triple) { $cleanArgs += @('--target', $triple) }
  cargo @cleanArgs
  if ($LASTEXITCODE -ne 0) { throw "cargo clean failed ($LASTEXITCODE)" }
}

# Fails unless the whisper.cpp cache exists and matches the baseline.
function Assert-GgmlBaseline([string]$manifest, [string]$arch, [string]$cargoProfile, [string]$triple) {
  $caches = @(Get-GgmlCaches (Get-GgmlProfileDir $manifest $cargoProfile $triple))
  if ($caches.Count -lt 1) { throw "Expected the whisper.cpp CMake cache (whisper-rs-sys), found none" }
  foreach ($c in $caches) {
    $bad = @(Test-GgmlBaseline $c $arch $cargoProfile)
    if ($bad.Count -gt 0) { throw "Not built for the $arch baseline: $c ($($bad -join '; '))" }
    Write-Host "OK: $c"
    Get-Content $c | Where-Object { $_ -match '^(GGML_(NATIVE|AVX[A-Z0-9_]*|FMA|F16C|BMI2|SSE42):BOOL|CMAKE_(C|CXX)_FLAGS_RELEASE:[A-Z]+)=' } |
      ForEach-Object { Write-Host "  $_" }
  }
}

function Get-GgmlProfileDir([string]$manifest, [string]$cargoProfile, [string]$triple) {
  $dir = Join-Path (Split-Path $manifest -Parent) 'target'
  if ($triple) { $dir = Join-Path $dir $triple }
  Join-Path $dir $(if ($cargoProfile -eq 'dev') { 'debug' } else { $cargoProfile })
}
