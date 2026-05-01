param(
    [string]$ConfigPath = "src-tauri\config.yaml",
    [string]$EnvFile = "office-addin\.env",
    [switch]$ShowToken
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

Write-Host "=== Office Hub Token Sync ===" -ForegroundColor Cyan
Write-Host ""

# --- Find config.yaml ---
$candidates = @(
    (Join-Path $env:APPDATA "office-hub\config.yaml"),
    (Join-Path $scriptDir $ConfigPath),
    (Join-Path $scriptDir "config.yaml"),
    (Join-Path $scriptDir "src-tauri\target\debug\config.yaml"),
    (Join-Path $scriptDir "src-tauri\target\release\config.yaml")
)

$foundConfig = $null
foreach ($c in $candidates) {
    if ($c -and (Test-Path $c)) { $foundConfig = $c; break }
}

if (-not $foundConfig) {
    Write-Host "WARN: config.yaml not found. Searched:" -ForegroundColor Yellow
    $candidates | ForEach-Object { if ($_) { Write-Host "   $_" } }
    Write-Host ""
    Write-Host "-> Start the Office Hub desktop app once so it generates config.yaml" -ForegroundColor Yellow
    exit 1
}

Write-Host "Found config: $foundConfig" -ForegroundColor Green

# --- Parse auth_secret from YAML ---
$configContent = Get-Content $foundConfig -Raw -Encoding UTF8
$tokenMatch = [regex]::Match($configContent, 'auth_secret:\s*[''"]?([a-f0-9\-]+)[''"]?')
if (-not $tokenMatch.Success) {
    Write-Host "ERROR: auth_secret not found in config.yaml" -ForegroundColor Red
    exit 1
}

$token = $tokenMatch.Groups[1].Value.Trim()
if ($ShowToken) {
    Write-Host "   Token: $token"
} else {
    Write-Host "   Token: $($token.Substring(0,8))...$($token.Substring($token.Length-4))"
}

# --- Write to office-addin/.env ---
$envFull = if ([System.IO.Path]::IsPathRooted($EnvFile)) { $EnvFile } else { Join-Path $scriptDir $EnvFile }

$envDir = Split-Path $envFull
if (-not (Test-Path $envDir)) { New-Item -ItemType Directory -Force -Path $envDir | Out-Null }

$envContent = ""
if (Test-Path $envFull) {
    $envContent = Get-Content $envFull -Raw -Encoding UTF8
    if ($envContent -match 'VITE_AUTH_TOKEN=') {
        $envContent = [regex]::Replace($envContent, 'VITE_AUTH_TOKEN=.*', "VITE_AUTH_TOKEN=$token")
    } else {
        $envContent += "`nVITE_AUTH_TOKEN=$token"
    }
} else {
    $envContent = "VITE_AUTH_TOKEN=$token`n"
}

$envContent | Set-Content $envFull -NoNewline -Encoding UTF8
Write-Host "Written to: $envFull" -ForegroundColor Green

# --- Instructions ---
Write-Host ""
Write-Host "Token sync complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Next steps:" -ForegroundColor White
Write-Host "  1. Restart add-in dev server: cd office-addin && npm run dev" -ForegroundColor Gray
Write-Host "  2. Re-sideload the add-in in Word/Excel/PowerPoint" -ForegroundColor Gray
Write-Host "  3. The taskpane should show 'Connected'" -ForegroundColor Gray
Write-Host ""

# --- Check backend connectivity ---
Write-Host "Checking backend on port 9001..." -ForegroundColor Cyan
$tcpTest = Test-NetConnection -ComputerName localhost -Port 9001 -WarningAction SilentlyContinue -InformationLevel Quiet
if ($tcpTest) {
    Write-Host "Backend WS server is reachable at localhost:9001" -ForegroundColor Green
} else {
    Write-Host "Backend NOT reachable at localhost:9001 - Start the app first" -ForegroundColor Red
}
