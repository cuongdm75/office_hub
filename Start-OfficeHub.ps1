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


# ─── STEP 3: Summary ─────────────────────────────────────────
Write-Host "`n============================================" -ForegroundColor White
Write-Host "  Office Hub is ready!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor White
Write-Host "  Backend  : ws://localhost:9001"
Write-Host "  Add-in UI: https://localhost:3000 (built-in)"
Write-Host ""
Write-Host "  The add-in HTTPS server is now embedded in Office Hub."
Write-Host "  No need to run `npm run dev` separately."
Write-Host ""
Write-Host "  If Outlook/Word ribbon icon is missing:"
Write-Host "    → Restart the Office app once after the first run"
Write-Host "    → Or re-run office-addin/Setup-OfficeAddin.ps1"
Write-Host "============================================`n" -ForegroundColor White
