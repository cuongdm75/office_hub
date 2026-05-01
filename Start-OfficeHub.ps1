# ============================================================
# Start-OfficeHub.ps1
# One-click startup script for Office Hub + Add-ins
# Run this once each session before using the add-in
# ============================================================

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$AddinDir    = Join-Path $ProjectRoot "office-addin"
$BackendExe  = Join-Path $ProjectRoot "src-tauri\target\debug\office-hub.exe"
$CertsDir    = "$env:USERPROFILE\.office-addin-dev-certs"

function Write-Step($msg) { Write-Host "`n[$((Get-Date).ToString('HH:mm:ss'))] $msg" -ForegroundColor Cyan }
function Write-OK($msg)   { Write-Host "  ✅ $msg" -ForegroundColor Green }
function Write-Fail($msg) { Write-Host "  ❌ $msg" -ForegroundColor Red }
function Write-Info($msg) { Write-Host "  ℹ  $msg" -ForegroundColor Yellow }

# ─── STEP 1: Install / trust dev certs ──────────────────────
Write-Step "Step 1: Ensuring dev certs are installed & trusted..."

$caPath = Join-Path $CertsDir "ca.crt"
if (-not (Test-Path $caPath)) {
    Write-Info "Dev certs not found, generating..."
    Push-Location $AddinDir
    npx office-addin-dev-certs install 2>&1 | Out-Null
    Pop-Location
}

# Check if cert is already trusted in BOTH stores
$thumbprint = $null
try {
    $tmpCert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($caPath)
    $thumbprint = $tmpCert.Thumbprint

    # 1. CurrentUser\Root (no admin)
    $existingCU = Get-ChildItem Cert:\CurrentUser\Root | Where-Object { $_.Thumbprint -eq $thumbprint }
    if (-not $existingCU) {
        $store = New-Object System.Security.Cryptography.X509Certificates.X509Store("Root","CurrentUser")
        $store.Open("ReadWrite")
        $store.Add($tmpCert)
        $store.Close()
        Write-OK "CA cert imported into CurrentUser\Root"
    } else {
        Write-OK "CA cert already in CurrentUser\Root"
    }

    # 2. LocalMachine\Root (requires admin - needed for WebView2 in Office)
    $existingLM = Get-ChildItem Cert:\LocalMachine\Root | Where-Object { $_.Thumbprint -eq $thumbprint }
    if (-not $existingLM) {
        Write-Info "Importing to LocalMachine\Root (UAC prompt may appear)..."
        $psCmd = "`$c=New-Object System.Security.Cryptography.X509Certificates.X509Certificate2('$caPath');`$s=New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','LocalMachine');`$s.Open('ReadWrite');`$s.Add(`$c);`$s.Close();Write-Host 'Done'"
        Start-Process powershell -ArgumentList "-NoProfile -ExecutionPolicy Bypass -Command $psCmd" -Verb RunAs -Wait
        $verify = Get-ChildItem Cert:\LocalMachine\Root | Where-Object { $_.Thumbprint -eq $thumbprint }
        if ($verify) { Write-OK "CA cert imported into LocalMachine\Root (WebView2 will trust HTTPS)" }
        else { Write-Fail "LocalMachine\Root import failed - UAC may have been declined" }
    } else {
        Write-OK "CA cert already in LocalMachine\Root"
    }
} catch {
    Write-Fail "Cert import failed: $_"
}

# ─── STEP 2: Start backend ──────────────────────────────────
Write-Step "Step 2: Starting Office Hub backend (port 9001)..."

# Kill any old instance
Get-Process -Name "office-hub" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

if (-not (Test-Path $BackendExe)) {
    Write-Fail "Backend not built! Run: cd src-tauri && cargo build"
    exit 1
}

Start-Process -FilePath $BackendExe -WorkingDirectory (Split-Path $BackendExe) -WindowStyle Hidden
Start-Sleep -Seconds 2

$listening = netstat -ano 2>$null | Select-String "0.0.0.0:9001.*LISTENING"
if ($listening) {
    Write-OK "Backend running on port 9001"
} else {
    Write-Fail "Backend failed to start on port 9001"
}

# ─── STEP 3: Start Vite dev server ──────────────────────────
Write-Step "Step 3: Starting Add-in Dev Server (https://localhost:3000)..."

# Kill old Vite on port 3000
$oldPid = (netstat -ano 2>$null | Select-String "3000.*LISTENING" | ForEach-Object { ($_ -split "\s+")[-1] } | Select-Object -First 1)
if ($oldPid) {
    Stop-Process -Id $oldPid -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

# Start Vite in background
$npmJob = Start-Job -ScriptBlock {
    param($dir)
    Set-Location $dir
    npm run dev 2>&1
} -ArgumentList $AddinDir

Start-Sleep -Seconds 5

# Verify port 3000 is up
$port3000 = netstat -ano 2>$null | Select-String "3000.*LISTENING"
if ($port3000) {
    Write-OK "Dev server running on https://localhost:3000"
} else {
    Write-Fail "Dev server failed to start. Check npm run dev manually."
}

# ─── STEP 4: Verify HTTPS reachable ─────────────────────────
Write-Step "Step 4: Verifying HTTPS connectivity..."
try {
    Add-Type -AssemblyName System.Net.Http
    $handler = New-Object System.Net.Http.HttpClientHandler
    $client  = New-Object System.Net.Http.HttpClient($handler)
    $task    = $client.GetAsync("https://localhost:3000/")
    $task.Wait(5000) | Out-Null
    if ($task.Result.StatusCode -eq "OK") {
        Write-OK "https://localhost:3000/ accessible ✓"
    }
} catch {
    Write-Fail "HTTPS check failed: $_"
    Write-Info "Office won't load the add-in if localhost is not HTTPS-accessible"
}

# ─── STEP 5: Summary ─────────────────────────────────────────
Write-Host "`n============================================" -ForegroundColor White
Write-Host "  Office Hub is ready!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor White
Write-Host "  Backend  : ws://localhost:9001"
Write-Host "  Add-in UI: https://localhost:3000"
Write-Host ""
Write-Host "  If Outlook/Word icon is missing:"
Write-Host "    → Restart the Office app after running this script"
Write-Host "    → Or run: npx office-addin-debugging start outlook-manifest.xml desktop --prod"
Write-Host "============================================`n" -ForegroundColor White
