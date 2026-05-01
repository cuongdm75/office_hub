param(
    [int]$AddinPort = 3000,
    [int]$BackendPort = 9001,
    [string]$ManifestPath = "office-addin\manifest.xml",
    [switch]$SkipCert,
    [switch]$SkipLaunch
)

$ErrorActionPreference = "Continue"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

Write-Host ""
Write-Host "=== Office Hub Add-in Connectivity Checker ===" -ForegroundColor Cyan
Write-Host ""

$passed = 0; $failed = 0; $warnings = 0

function Check-Ok([string]$msg) {
    Write-Host "  OK  $msg" -ForegroundColor Green
    $script:passed++
}
function Check-Fail([string]$msg, [string]$hint = "") {
    Write-Host "  FAIL  $msg" -ForegroundColor Red
    if ($hint) { Write-Host "     -> $hint" -ForegroundColor Yellow }
    $script:failed++
}
function Check-Warn([string]$msg, [string]$hint = "") {
    Write-Host "  WARN  $msg" -ForegroundColor Yellow
    if ($hint) { Write-Host "     -> $hint" -ForegroundColor Gray }
    $script:warnings++
}

# --- 1. Backend WS server ---
Write-Host "[1/7] Backend WebSocket Server (port $BackendPort)" -ForegroundColor White
$ws = Test-NetConnection -ComputerName localhost -Port $BackendPort -WarningAction SilentlyContinue -InformationLevel Quiet
if ($ws) { Check-Ok "Backend reachable at localhost:$BackendPort" }
else { Check-Fail "Backend NOT reachable at localhost:$BackendPort" "Start the Office Hub desktop app first" }

# --- 2. Auth Token Sync ---
Write-Host ""
Write-Host "[2/7] Auth Token Sync" -ForegroundColor White

$foundConfig = $null
foreach ($candidate in @(
    (Join-Path $env:APPDATA "office-hub\config.yaml"),
    (Join-Path $scriptDir "src-tauri\config.yaml"),
    (Join-Path $scriptDir "config.yaml"),
    (Join-Path $scriptDir "src-tauri\target\debug\config.yaml")
)) {
    if (Test-Path $candidate) { $foundConfig = $candidate; break }
}

if ($foundConfig) {
    $cfg = Get-Content $foundConfig -Raw
    $match = [regex]::Match($cfg, 'auth_secret:\s*[''"]?([a-f0-9\-]+)[''"]?')
    if ($match.Success) {
        $token = $match.Groups[1].Value.Trim()
        Check-Ok "Backend token found ($($token.Substring(0,8))...)"

        $envPath = Join-Path $scriptDir "office-addin\.env"
        if (Test-Path $envPath) {
            $envContent = Get-Content $envPath -Raw
            if ($envContent -match "VITE_AUTH_TOKEN=$token") { Check-Ok "office-addin/.env token is in sync" }
            else { Check-Fail "office-addin/.env token OUT OF SYNC" "Run: .\sync_token.ps1" }
        } else {
            Check-Fail "office-addin/.env missing" "Run: .\sync_token.ps1"
        }
    } else {
        Check-Fail "auth_secret not found in config.yaml" "Start the app once to generate it"
    }
} else {
    Check-Fail "config.yaml not found" "Start the desktop app once to generate it"
}

# --- 3. HTTPS Dev Certificates ---
Write-Host ""
Write-Host "[3/7] HTTPS Dev Certificates" -ForegroundColor White

if ($SkipCert) {
    Check-Warn "Certificate check skipped" "Cert is required for Office Add-ins"
} else {
    $certDir = Join-Path $env:USERPROFILE ".office-addin-dev-certs"
    if (Test-Path (Join-Path $certDir "localhost.key")) { Check-Ok "localhost.key found" }
    else { Check-Fail "localhost.key NOT found" "Run: npx office-addin-dev-certs install" }

    if (Test-Path (Join-Path $certDir "localhost.crt")) { Check-Ok "localhost.crt found" }
    else { Check-Fail "localhost.crt NOT found" "Run: npx office-addin-dev-certs install" }
}

# --- 4. Add-in dev server ---
Write-Host ""
Write-Host "[4/7] Add-in Dev Server (port $AddinPort)" -ForegroundColor White

$addinUp = Test-NetConnection -ComputerName localhost -Port $AddinPort -WarningAction SilentlyContinue -InformationLevel Quiet
if ($addinUp) {
    Check-Ok "Add-in server running at localhost:$AddinPort"
    try {
        $resp = Invoke-WebRequest "https://localhost:$AddinPort/" -SkipCertificateCheck -TimeoutSec 5 -ErrorAction Stop
        if ($resp.StatusCode -eq 200) { Check-Ok "HTTPS page loads (status 200)" }
        else { Check-Warn "Add-in page returned status $($resp.StatusCode)" }
    } catch { Check-Warn "Could not verify HTTPS page (cert may need trust)" }
} else {
    Check-Fail "Add-in server NOT running on port $AddinPort" "Run: cd office-addin && npm run dev"
}

# --- 5. Manifest file ---
Write-Host ""
Write-Host "[5/7] Manifest File" -ForegroundColor White

$manifestFull = Join-Path $scriptDir $ManifestPath
if (Test-Path $manifestFull) {
    Check-Ok "manifest.xml found"
    $manifest = Get-Content $manifestFull -Raw
    if ($manifest -match "localhost:$AddinPort") { Check-Ok "Manifest points to port $AddinPort" }
    else { Check-Warn "Manifest may not point to port $AddinPort" }
    if ($manifest -match "Document")      { Check-Ok "Word host configured" }
    if ($manifest -match "Workbook")      { Check-Ok "Excel host configured" }
    if ($manifest -match "Presentation")  { Check-Ok "PowerPoint host configured" }
} else {
    Check-Fail "manifest.xml not found at $manifestFull"
}

# --- 6. Node modules ---
Write-Host ""
Write-Host "[6/7] Dependencies" -ForegroundColor White

if (Test-Path (Join-Path $scriptDir "office-addin\node_modules")) { Check-Ok "office-addin/node_modules exists" }
else { Check-Fail "office-addin/node_modules missing" "Run: cd office-addin && npm install" }

# --- 7. Sideload instructions ---
Write-Host ""
Write-Host "[7/7] Sideload Instructions" -ForegroundColor White
Write-Host "  1. Open Word / Excel / PowerPoint" -ForegroundColor Gray
Write-Host "  2. Insert -> Add-ins -> My Add-ins -> Upload My Add-in" -ForegroundColor Gray
Write-Host "  3. Select: $manifestFull" -ForegroundColor Gray
Write-Host "  4. Click 'Office Hub AI' in the Home ribbon" -ForegroundColor Gray
Write-Host "  5. Taskpane should show 'Connected'" -ForegroundColor Gray

# --- Summary ---
Write-Host ""
Write-Host "--- Summary ---" -ForegroundColor White
Write-Host "  Passed:   $passed" -ForegroundColor Green
Write-Host "  Failed:   $failed" -ForegroundColor $(if ($failed -gt 0) { "Red" } else { "Green" })
Write-Host "  Warnings: $warnings" -ForegroundColor $(if ($warnings -gt 0) { "Yellow" } else { "White" })
Write-Host ""

if ($failed -eq 0) {
    Write-Host "All checks passed! Add-in should be working." -ForegroundColor Green
} else {
    Write-Host "$failed check(s) failed. Fix the issues above before testing the add-in." -ForegroundColor Red
    exit 1
}
