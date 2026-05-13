param(
  [string]$ProjectPath = "\\ST-HomeNAS\DataTransFile\4isfstools",
  [switch]$PortableOnly
)

$ErrorActionPreference = "Stop"

function Step($msg) { Write-Host "`n=== $msg ===" -ForegroundColor Cyan }
function Ok($msg)   { Write-Host "[OK] $msg" -ForegroundColor Green }
function Warn($msg) { Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "[FAIL] $msg" -ForegroundColor Red; exit 1 }

Step "检查项目目录"
if (-not (Test-Path $ProjectPath)) { Fail "找不到项目目录: $ProjectPath" }
Set-Location $ProjectPath
Ok "当前目录: $(Get-Location)"

Step "检查工具链"
$tools = @("node", "npm", "cargo", "rustc")
foreach ($t in $tools) {
  if (-not (Get-Command $t -ErrorAction SilentlyContinue)) {
    Fail "缺少命令: $t，请先安装对应环境"
  }
}
Ok "Node: $(node -v)"
Ok "NPM : $(npm -v)"
Ok "Rust: $(rustc -V)"
Ok "Cargo: $(cargo -V)"

Step "安装前端依赖 (npm ci)"
npm ci
if ($LASTEXITCODE -ne 0) { Fail "npm ci 失败" }
Ok "依赖安装完成"

Step "Rust 检查 (cargo check)"
cargo check
if ($LASTEXITCODE -ne 0) { Fail "cargo check 失败" }
Ok "cargo check 通过"

Step "前端构建 (npm run build)"
npm run build
if ($LASTEXITCODE -ne 0) { Fail "npm run build 失败" }
Ok "前端构建通过"

if ($PortableOnly) {
  Step "打 Windows 便携包 (npm run win:portable)"
  npm run win:portable
  if ($LASTEXITCODE -ne 0) { Fail "win:portable 失败" }
} else {
  Step "打 Tauri Windows 包 (npm run tauri build)"
  npm run tauri build
  if ($LASTEXITCODE -ne 0) { Fail "tauri build 失败" }

  Step "额外生成便携包 (npm run win:portable)"
  npm run win:portable
  if ($LASTEXITCODE -ne 0) { Warn "win:portable 失败（主安装包可能已成功）" }
}

Step "查找产物"
$paths = @(
  "src-tauri\target\release\bundle",
  "dist",
  "release",
  "artifacts"
)

foreach ($p in $paths) {
  $full = Join-Path $ProjectPath $p
  if (Test-Path $full) {
    Ok "产物目录: $full"
  }
}

Get-ChildItem -Path $ProjectPath -Recurse -File -Include *.msi,*.exe,*.zip `
  -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 20 FullName, Length, LastWriteTime |
  Format-Table -AutoSize

Ok "编译流程结束"
