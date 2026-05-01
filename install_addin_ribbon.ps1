param(
    [string]$ManifestDir = "E:\Office hub\office-addin-catalog",
    [switch]$Remove
)

$manifestSource = "E:\Office hub\office-addin\manifest.xml"

Write-Host ""
Write-Host "=== Office Hub AI - Ribbon Installer ===" -ForegroundColor Cyan
Write-Host ""

if ($Remove) {
    # --- Uninstall ---
    $regKey = "HKCU:\SOFTWARE\Microsoft\Office\16.0\WEF\TrustedCatalogs"
    $catalogId = "{3a4a1c5d-0000-0000-0000-000000000001}"
    if (Test-Path "$regKey\$catalogId") {
        Remove-Item "$regKey\$catalogId" -Recurse -Force
        Write-Host "Removed Office Hub from trusted catalogs" -ForegroundColor Green
    }
    Write-Host "Restart Word/Excel/PowerPoint to apply." -ForegroundColor Yellow
    exit 0
}

# --- Step 1: Create catalog folder ---
Write-Host "[1/4] Creating catalog folder..." -ForegroundColor White
if (-not (Test-Path $ManifestDir)) {
    New-Item -ItemType Directory -Force -Path $ManifestDir | Out-Null
    Write-Host "  Created: $ManifestDir" -ForegroundColor Green
} else {
    Write-Host "  Exists: $ManifestDir" -ForegroundColor Green
}

# --- Step 2: Copy manifest ---
Write-Host "[2/4] Copying manifest..." -ForegroundColor White
if (Test-Path $manifestSource) {
    Copy-Item $manifestSource "$ManifestDir\manifest.xml" -Force
    Write-Host "  Copied manifest.xml" -ForegroundColor Green
} else {
    Write-Host "  ERROR: manifest.xml not found at $manifestSource" -ForegroundColor Red
    exit 1
}

# --- Step 3: Register in Office Trusted Catalog (registry) ---
Write-Host "[3/4] Registering in Office Trusted Catalog..." -ForegroundColor White

$catalogId = "{3a4a1c5d-0000-0000-0000-000000000001}"
$regRoot   = "HKCU:\SOFTWARE\Microsoft\Office\16.0\WEF\TrustedCatalogs"

if (-not (Test-Path $regRoot)) {
    New-Item -Path $regRoot -Force | Out-Null
}

$regPath = "$regRoot\$catalogId"
if (-not (Test-Path $regPath)) {
    New-Item -Path $regPath -Force | Out-Null
}

Set-ItemProperty -Path $regPath -Name "Id"          -Value $catalogId
Set-ItemProperty -Path $regPath -Name "Url"         -Value $ManifestDir
Set-ItemProperty -Path $regPath -Name "Flags"       -Value 1 -Type DWord
Set-ItemProperty -Path $regPath -Name "CatalogType" -Value 1 -Type DWord

Write-Host "  Registered catalog: $ManifestDir" -ForegroundColor Green
Write-Host "  Registry: $regPath" -ForegroundColor Gray

# --- Step 4: Icon files check ---
Write-Host "[4/4] Checking icon files..." -ForegroundColor White
$iconDir = "E:\Office hub\office-addin\public"
$icons = @("icon-16.png", "icon-32.png", "icon-80.png", "icon-128.png", "icon-64.png")
$missingIcons = @()
foreach ($icon in $icons) {
    $iconPath = Join-Path $iconDir $icon
    if (-not (Test-Path $iconPath)) {
        $missingIcons += $icon
    }
}

if ($missingIcons.Count -gt 0) {
    Write-Host "  WARN: Missing icon files: $($missingIcons -join ', ')" -ForegroundColor Yellow
    Write-Host "  -> Will generate placeholder icons..." -ForegroundColor Yellow
    
    # Generate simple placeholder PNG icons using System.Drawing
    Add-Type -AssemblyName System.Drawing
    foreach ($icon in $missingIcons) {
        $sizePart = $icon -replace "icon-", "" -replace ".png", ""
        $size = [int]$sizePart
        if ($size -lt 16) { $size = 16 }
        
        $bmp = New-Object System.Drawing.Bitmap($size, $size)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.Clear([System.Drawing.Color]::FromArgb(0, 120, 212))
        $font = New-Object System.Drawing.Font("Arial", [Math]::Max(6, $size/4), [System.Drawing.FontStyle]::Bold)
        $brush = [System.Drawing.Brushes]::White
        $g.DrawString("AI", $font, $brush, 0, $size/4)
        $g.Dispose()
        $savePath = Join-Path $iconDir $icon
        $bmp.Save($savePath)
        $bmp.Dispose()
        Write-Host "  Generated: $icon ($size x $size)" -ForegroundColor Gray
    }
} else {
    Write-Host "  All icon files found" -ForegroundColor Green
}

# --- Done ---
Write-Host ""
Write-Host "=== Installation Complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "IMPORTANT: You must restart Word/Excel/PowerPoint for the button to appear." -ForegroundColor Yellow
Write-Host ""
Write-Host "After restarting Office:" -ForegroundColor White
Write-Host "  1. Go to Insert tab -> Get Add-ins (or My Add-ins)" -ForegroundColor Gray
Write-Host "  2. Click 'MY ADD-INS' tab" -ForegroundColor Gray
Write-Host "  3. Click 'SHARED FOLDER' tab" -ForegroundColor Gray
Write-Host "  4. Select 'Office Hub AI' and click Add" -ForegroundColor Gray
Write-Host "  5. The 'Office Hub AI' button appears in the Home ribbon tab" -ForegroundColor Gray
Write-Host ""
Write-Host "Or use the quick method:" -ForegroundColor White
Write-Host "  Home tab -> look for 'Office Hub' group on the RIGHT side" -ForegroundColor Gray
Write-Host ""
Write-Host "Make sure the add-in server is running:" -ForegroundColor White  
Write-Host "  cd E:\Office hub\office-addin && npm run dev" -ForegroundColor Gray
Write-Host ""
