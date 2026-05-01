# Setup-OfficeAddin.ps1
# Sideload vinh vien Office Add-in thong qua Developer Registry
# Phien ban: 2.0 - Fix SSL WebView2 + Loopback

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$currentPath = Split-Path -Parent $MyInvocation.MyCommand.Definition
$registryPath = "HKCU:\Software\Microsoft\Office\16.0\WEF\Developer"

Write-Host "`n=== Office Hub Add-in Setup ===" -ForegroundColor Cyan

# --- Step 1: Cai dat chung chi Dev ---
Write-Host "`n[1/5] Cai dat chung chi Office Add-in Dev (neu chua co)..." -ForegroundColor Cyan
npx office-addin-dev-certs install 2>&1 | Out-Null
$caPath = Join-Path $env:USERPROFILE ".office-addin-dev-certs\ca.crt"

if (-not (Test-Path $caPath)) {
    Write-Host "[ERR] Khong tim thay CA cert tai $caPath" -ForegroundColor Red
    Write-Host "      Chay: npx office-addin-dev-certs install" -ForegroundColor Yellow
    exit 1
}
Write-Host "[OK] Dev certs da ton tai." -ForegroundColor Green

# --- Step 2: Cai CA cert vao LocalMachine\Root (WebView2 yeu cau) ---
Write-Host "`n[2/5] Cai CA cert vao LocalMachine\Root (WebView2 can de load HTTPS)..." -ForegroundColor Cyan

# Kiem tra xem da co chua
$existingCert = Get-ChildItem Cert:\LocalMachine\Root | Where-Object { $_.Subject -match "Developer CA for Microsoft Office Add-ins" }
if ($existingCert) {
    Write-Host "[OK] CA cert da duoc tin tuong trong LocalMachine\Root." -ForegroundColor Green
} else {
    Write-Host "     Dang cai vao LocalMachine\Root (can quyen Admin)..." -ForegroundColor Yellow
    $psCmd = @"
`$c = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2('$caPath')
`$s = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','LocalMachine')
`$s.Open('ReadWrite')
`$s.Add(`$c)
`$s.Close()
Write-Host '[OK] CA cert da duoc them vao LocalMachine\Root.'
"@
    try {
        Start-Process powershell -ArgumentList "-NoProfile -ExecutionPolicy Bypass -Command `"$psCmd`"" -Verb RunAs -Wait
        Write-Host "[OK] CA cert da duoc them vao LocalMachine\Root." -ForegroundColor Green
    } catch {
        Write-Host "[WARN] Khong the chay Admin: $_" -ForegroundColor Yellow
        Write-Host "       Vui long chay thu cong voi quyen Admin:" -ForegroundColor Yellow
        Write-Host "       certutil -addstore Root `"$caPath`"" -ForegroundColor Gray
    }
}

# --- Step 3: Cap quyen Loopback cho Edge WebView2 ---
Write-Host "`n[3/5] Cap quyen Loopback cho Edge WebView2 (cho phep ws://127.0.0.1)..." -ForegroundColor Cyan
$loopbackResult = CheckNetIsolation LoopbackExempt -a -n="microsoft.win32webviewhost_cw5n1h2txyewy" 2>&1
if ($LASTEXITCODE -eq 0 -or $loopbackResult -match "OK|success|The operation completed") {
    Write-Host "[OK] WebView2 loopback exempt da duoc cap phep." -ForegroundColor Green
} else {
    Write-Host "[WARN] Loopback exempt co the da ton tai hoac can quyen cao hon." -ForegroundColor Yellow
}

# Also add exemption for Edge (for newer WebView2 runtime)
CheckNetIsolation LoopbackExempt -a -n="microsoft.microsoftedge.stable_8wekyb3d8bbwe" 2>&1 | Out-Null

# --- Step 4: Dang ky manifest qua Developer Registry ---
Write-Host "`n[4/5] Dang ky manifest qua Developer Registry..." -ForegroundColor Cyan

if (-not (Test-Path $registryPath)) {
    New-Item -Path $registryPath -Force | Out-Null
}

$mainManifest = Join-Path $currentPath "manifest.xml"
$outlookManifest = Join-Path $currentPath "outlook-manifest.xml"

if (Test-Path $mainManifest) {
    Set-ItemProperty -Path $registryPath -Name "3a4a1c5d-8b0f-48d9-a477-8e6fcb5b9c2a" -Value $mainManifest
    Write-Host "[OK] manifest.xml da duoc dang ky (Word, Excel, PowerPoint)." -ForegroundColor Green
} else {
    Write-Host "[WARN] Khong tim thay manifest.xml tai: $mainManifest" -ForegroundColor Yellow
}

if (Test-Path $outlookManifest) {
    Set-ItemProperty -Path $registryPath -Name "60a9434f-bc3b-445f-a16d-b66b33c75b8b" -Value $outlookManifest
    Write-Host "[OK] outlook-manifest.xml da duoc dang ky (Outlook)." -ForegroundColor Green
} else {
    Write-Host "[WARN] Khong tim thay outlook-manifest.xml tai: $outlookManifest" -ForegroundColor Yellow
}

# --- Step 5: Xac minh ket qua ---
Write-Host "`n[5/5] Xac minh cai dat..." -ForegroundColor Cyan

# Check port 3000
$port3000 = netstat -ano 2>&1 | Select-String ":3000"
if ($port3000) {
    Write-Host "[OK] Dev server dang chay tren port 3000." -ForegroundColor Green
} else {
    Write-Host "[WARN] Port 3000 chua duoc mo. Hay chay: npm run dev" -ForegroundColor Yellow
}

# Check port 9001
$port9001 = netstat -ano 2>&1 | Select-String ":9001"
if ($port9001) {
    Write-Host "[OK] Office Hub backend WS dang chay tren port 9001." -ForegroundColor Green
} else {
    Write-Host "[WARN] Port 9001 chua duoc mo. Hay khoi dong ung dung Office Hub." -ForegroundColor Yellow
}

Write-Host "`n======================================" -ForegroundColor Cyan
Write-Host " CAI DAT HOAN TAT!" -ForegroundColor Green
Write-Host "======================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Buoc tiep theo:" -ForegroundColor White
Write-Host "  1. Dam bao Office Hub (Tauri app) dang chay" -ForegroundColor Yellow
Write-Host "  2. Dam bao dev server dang chay: npm run dev (trong office-addin/)" -ForegroundColor Yellow
Write-Host "  3. Khoi dong lai Word / Excel / PowerPoint / Outlook" -ForegroundColor Yellow
Write-Host "  4. Add-in 'Office Hub AI' se xuat hien tren thanh Ribbon" -ForegroundColor Yellow
Write-Host ""
