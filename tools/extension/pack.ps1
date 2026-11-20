# Builds the browser extension for each browser family from extension/ (one background.js):
#   target/extension/chromium/  - manifest.json (Chrome, Edge, Brave, Opera, Vivaldi; load unpacked)
#   target/extension/firefox/   - manifest.firefox.json as manifest.json
#   target/extension/odm-chromium.zip - the chromium folder, zipped (a release download: unzip it
#                                       and load the folder unpacked)
#   target/extension/odm-firefox.xpi  - the firefox folder, zipped (Firefox needs it signed by AMO
#                                       to install for good; about:debugging loads it unsigned)
#   target/extension/odm-chrome-store.zip - what is uploaded to the Chrome Web Store (see below)
# Both manifests get the app's version (Cargo.toml), so an extension and its app always match.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$src = Join-Path $root "extension"
$out = Join-Path (Join-Path $root "target") "extension"
# Only generated folders below this workspace's target/extension may be replaced.
$out = [IO.Path]::GetFullPath($out)
if (-not $out.StartsWith(([IO.Path]::GetFullPath($root) + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
  throw "extension output must remain inside the workspace"
}
function Remove-BuildFolder([string]$folder) {
  $resolved = [IO.Path]::GetFullPath($folder)
  if (-not $resolved.StartsWith(($out + [IO.Path]::DirectorySeparatorChar), [StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to remove a folder outside extension output"
  }
  if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
$cargo = Get-Content (Join-Path $root "Cargo.toml") -Raw
$version = [regex]::Match($cargo, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw "no version in Cargo.toml" }
# The stores take one to four dot-separated numbers only (no "-rc.1").
if ($version -notmatch '^\d+(\.\d+){0,3}$') { throw "version ${version}: browsers take only numbers like 1.2.3" }
foreach ($kind in "chromium", "firefox") {
  $dir = Join-Path $out $kind
  Remove-BuildFolder $dir
  New-Item -ItemType Directory -Force $dir | Out-Null
  # The background part; the download button over videos and the skip-key watcher (content
  # scripts in every page); the toolbar button's popup.
  foreach ($f in "background.js", "video.js", "page.js", "popup.html", "popup.css", "popup.js") {
    Copy-Item (Join-Path $src $f) $dir
  }
  foreach ($folder in "icons", "assets", "fonts") {
    Copy-Item (Join-Path $src $folder) (Join-Path $dir $folder) -Recurse
  }
  # Ship English texts only, regardless of the browser’s display language.
  $english = Join-Path (Join-Path $dir "_locales") "en"
  New-Item -ItemType Directory -Force $english | Out-Null
  Copy-Item (Join-Path $src "_locales/en/messages.json") (Join-Path $english "messages.json")
  $manifest = switch ($kind) { "firefox" { "manifest.firefox.json" } default { "manifest.json" } }
  $json = Get-Content (Join-Path $src $manifest) -Raw
  # The first "version" only (the manifest's own).
  $json = ([regex]'("version"\s*:\s*)"[^"]*"').Replace($json, "`${1}`"$version`"", 1)
  [IO.File]::WriteAllText((Join-Path $dir "manifest.json"), $json, (New-Object Text.UTF8Encoding $false))
}
# The Chrome Web Store's copy of the chromium folder. The store signs it with its own key (it
# refuses a manifest that carries "key"; the native host knows both IDs).
$store = Join-Path $out "chrome-store"
Remove-BuildFolder $store
Copy-Item (Join-Path $out "chromium") $store -Recurse
$manifest = Get-Content (Join-Path $store "manifest.json") -Raw | ConvertFrom-Json
$manifest.PSObject.Properties.Remove("key")
$json = $manifest | ConvertTo-Json -Depth 10
[IO.File]::WriteAllText((Join-Path $store "manifest.json"), $json, (New-Object Text.UTF8Encoding $false))
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
# The zips have manifest.json at their root. Entries are named with "/" as zip requires
# (Windows PowerShell's CreateFromDirectory writes "\", which Firefox rejects).
foreach ($pair in @(@("chromium", "odm-chromium.zip"), @("firefox", "odm-firefox.xpi"), @("chrome-store", "odm-chrome-store.zip"))) {
  $dir = (Resolve-Path (Join-Path $out $pair[0])).Path
  $zip = Join-Path $out $pair[1]
  if (Test-Path $zip) { Remove-Item -LiteralPath $zip -Force }
  $archive = [System.IO.Compression.ZipFile]::Open($zip, [System.IO.Compression.ZipArchiveMode]::Create)
  try {
    foreach ($f in Get-ChildItem -LiteralPath $dir -Recurse -File | Sort-Object FullName) {
      $name = $f.FullName.Substring($dir.Length).TrimStart('\', '/').Replace('\', '/')
      [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $f.FullName, $name)
    }
  } finally { $archive.Dispose() }
}
"built $out (chromium, firefox, chrome-store, odm-chromium.zip, odm-firefox.xpi, odm-chrome-store.zip; version $version)"
