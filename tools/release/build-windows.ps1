# Build a dated Orbit release with the installed Windows C++ toolchain.
param(
  [datetime]$Date = (Get-Date),
  [ValidateSet("x64", "arm64", "x86")][string]$Arch = "x64",
  [switch]$InstallerOnly,
  [switch]$NoBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location -LiteralPath $root
& node (Join-Path $PSScriptRoot "set-version.mjs") $Date.ToString("yyyy-MM-dd")
if ($LASTEXITCODE) { throw "setting the release version failed" }

if (-not $NoBuild) {
  $vswhere = Join-Path ([Environment]::GetFolderPath("ProgramFilesX86")) "Microsoft Visual Studio\Installer\vswhere.exe"
  if (-not (Test-Path -LiteralPath $vswhere)) { throw "Install Visual Studio C++ Build Tools and a Windows SDK first" }
  $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if (-not $vs) { throw "Visual Studio C++ Build Tools are not installed" }
  $dev = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
  $devArch = @{ x64 = "amd64"; arm64 = "arm64"; x86 = "x86" }[$Arch]
  $quote = [char]34
  $devCommand = $quote + $quote + $dev + $quote + " -arch=$devArch -host_arch=amd64 >nul && set" + $quote
  $environment = & $env:ComSpec /d /s /c $devCommand
  if ($LASTEXITCODE) { throw "initializing the Visual C++ build environment failed" }
  foreach ($entry in $environment) {
    if ($entry -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], "Process") }
  }
  # Scoop's LLVM includes libclang, used by BoringSSL/AWS-LC's Rust bindings.
  if (-not $env:LIBCLANG_PATH) {
    $llvm = Join-Path $env:USERPROFILE "scoop\apps\llvm\current\bin"
    if (Test-Path -LiteralPath (Join-Path $llvm "libclang.dll")) { $env:LIBCLANG_PATH = $llvm }
  }
  foreach ($tool in "cl.exe", "link.exe", "cmake.exe", "nasm.exe") {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing build dependency: $tool" }
  }
}

& (Join-Path $root "tools\installer\build.ps1") -Arch $Arch -NoBuild:$NoBuild -InstallerOnly:$InstallerOnly
if ($LASTEXITCODE) { throw "Windows packaging failed" }
& node (Join-Path $root "tools\extension\check-pack.mjs")
if ($LASTEXITCODE) { throw "extension validation failed" }
$version = [regex]::Match((Get-Content (Join-Path $root "Cargo.toml") -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$dist = Join-Path $root "target\dist"
Copy-Item -LiteralPath (Join-Path $root "target\extension\odm-chromium.zip") -Destination (Join-Path $dist "OrbitDownloadManager-$version-extension-chromium.zip")
Copy-Item -LiteralPath (Join-Path $root "target\extension\odm-firefox.xpi") -Destination (Join-Path $dist "OrbitDownloadManager-$version-extension-firefox.xpi")
Copy-Item -LiteralPath (Join-Path $root "target\extension\odm-chrome-store.zip") -Destination (Join-Path $dist "OrbitDownloadManager-$version-extension-chrome-store.zip")
$files = Get-ChildItem -LiteralPath $dist -File | Where-Object Name -Like "OrbitDownloadManager-$version-*" | Sort-Object Name
$files | ForEach-Object { "{0}  {1}" -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name } |
  Set-Content -Encoding ascii (Join-Path $dist "SHA256SUMS.txt")
$files | Select-Object Name,Length
