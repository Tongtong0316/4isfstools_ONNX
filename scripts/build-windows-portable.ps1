$ErrorActionPreference = 'Stop'

$projectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $projectRoot

Write-Host '[forisfstools] building frontend...'
npm run build

Write-Host '[forisfstools] building tauri executable (no installer)...'
npm run tauri build -- --no-bundle

$exeCandidates = @(
  (Join-Path $projectRoot 'src-tauri\target\release\forisfstools.exe'),
  (Join-Path $env:LOCALAPPDATA 'banzou-master\cargo-target\release\forisfstools.exe')
)

$exePath = $null
foreach ($candidate in $exeCandidates) {
  if (Test-Path $candidate) {
    $exePath = $candidate
    break
  }
}

if (-not $exePath) {
  throw 'Cannot find forisfstools.exe after build.'
}

$portableRoot = Join-Path $projectRoot 'dist-portable'
$packageDir = Join-Path $portableRoot 'Macaron Singer Portable'
$resourcesDir = Join-Path $packageDir 'resources\python'

if (Test-Path $packageDir) {
  Remove-Item -Recurse -Force $packageDir
}
New-Item -ItemType Directory -Force -Path $resourcesDir | Out-Null

Copy-Item $exePath (Join-Path $packageDir 'forisfstools.exe') -Force

$runtimeArchive = Join-Path $projectRoot 'python\python-standalone.tar.gz'
if (Test-Path $runtimeArchive) {
  Copy-Item $runtimeArchive (Join-Path $resourcesDir 'python-standalone.tar.gz') -Force
}

$zipPath = Join-Path $portableRoot 'Macaron-Singer-Windows-Portable.zip'
if (Test-Path $zipPath) {
  Remove-Item -Force $zipPath
}

Compress-Archive -Path (Join-Path $packageDir '*') -DestinationPath $zipPath -Force

Write-Host '[forisfstools] portable package ready:'
Write-Host "  $zipPath"
Write-Host ''
Write-Host 'Use on Windows:'
Write-Host '  1) unzip'
Write-Host '  2) run forisfstools.exe'
Write-Host '  3) open Preferences -> Dependencies & Model -> Install runtime'
