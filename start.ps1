param(
    [switch]$Stop,
    [switch]$Status
)

$root = "E:\Office hub"
$logFile = "$root\logs\startup.log"
$null = New-Item -ItemType Directory -Force -Path "$root\logs"

if ($Status) {
    Write-Host "=== Office Hub Status ===" -ForegroundColor Cyan
    $vite   = netstat -ano | findstr ":1420" | findstr "LISTENING"
    $ws     = netstat -ano | findstr ":9001" | findstr "LISTENING"
    $addin  = netstat -ano | findstr ":3000" | findstr "LISTENING"
    $mobile = netstat -ano | findstr ":8081" | findstr "LISTENING"
    Write-Host "  Desktop UI  (1420): $(if ($vite)  { 'RUNNING' } else { 'STOPPED' })" -ForegroundColor $(if ($vite)  { 'Green' } else { 'Red' })
    Write-Host "  Backend WS  (9001): $(if ($ws)    { 'RUNNING' } else { 'STOPPED' })" -ForegroundColor $(if ($ws)    { 'Green' } else { 'Red' })
    Write-Host "  Add-in Dev  (3000): $(if ($addin) { 'RUNNING' } else { 'STOPPED' })" -ForegroundColor $(if ($addin) { 'Green' } else { 'Red' })
    Write-Host "  Mobile Expo (8081): $(if ($mobile){ 'RUNNING' } else { 'STOPPED' })" -ForegroundColor $(if ($mobile){ 'Green' } else { 'Red' })
    exit 0
}

if ($Stop) {
    Write-Host "Stopping Office Hub..." -ForegroundColor Yellow
    Get-Process -Name "office-hub" -ErrorAction SilentlyContinue | Stop-Process -Force
    # Kill Vite dev server on port 1420
    $pid1420 = (netstat -ano | findstr ":1420" | findstr "LISTENING") -replace '.*LISTENING\s+', ''
    if ($pid1420) { Stop-Process -Id $pid1420.Trim() -Force -ErrorAction SilentlyContinue }
    Write-Host "Stopped." -ForegroundColor Green
    exit 0
}

# ── START ────────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== Starting Office Hub ===" -ForegroundColor Cyan
Write-Host ""

# 1. Kill any stale processes
Write-Host "[1/3] Cleaning up stale processes..." -ForegroundColor White
Get-Process -Name "office-hub" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1

# 2. Start Tauri dev (Vite + Rust backend together)
Write-Host "[2/3] Launching Desktop App (Vite + Tauri)..." -ForegroundColor White
Write-Host "      -> Vite:   http://localhost:1420" -ForegroundColor Gray
Write-Host "      -> WS:     ws://localhost:9001" -ForegroundColor Gray
$tauriJob = Start-Process "cmd.exe" -ArgumentList "/c npm run tauri:dev >> `"$logFile`" 2>&1" -WorkingDirectory $root -PassThru -WindowStyle Minimized
Write-Host "  PID: $($tauriJob.Id)" -ForegroundColor Green

# 3. Start Add-in dev server
Write-Host "[3/3] Launching Add-in Dev Server..." -ForegroundColor White
Write-Host "      -> Add-in: https://localhost:3000" -ForegroundColor Gray
$addinRunning = netstat -ano | findstr ":3000" | findstr "LISTENING"
if (-not $addinRunning) {
    $addinJob = Start-Process "cmd.exe" -ArgumentList "/c npm run dev >> `"$logFile`" 2>&1" -WorkingDirectory "$root\office-addin" -PassThru -WindowStyle Minimized
    Write-Host "  PID: $($addinJob.Id)" -ForegroundColor Green
} else {
    Write-Host "  Already running" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Waiting for services to start... ===" -ForegroundColor Yellow
Start-Sleep 8

# Show status
& $PSCommandPath -Status

Write-Host ""
Write-Host "Logs: $logFile" -ForegroundColor Gray
Write-Host ""
Write-Host "Add-in sideload (in Word/Excel/PowerPoint):" -ForegroundColor White
Write-Host "  Insert -> Get Add-ins -> MY ADD-INS -> SHARED FOLDER -> Office Hub AI" -ForegroundColor Gray
Write-Host ""
