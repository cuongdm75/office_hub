# Create icon.ico for Tauri Windows build
# This script generates a simple 32x32 blue icon

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

# Create a 32x32 bitmap
$bmp = New-Object System.Drawing.Bitmap(32, 32)
$g = [System.Drawing.Graphics]::FromImage($bmp)

# Clear with blue color (Tauri default-ish)
$g.Clear([System.Drawing.Color]::FromArgb(0, 120, 215))

# Draw a simple circle in the center
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 2)
$g.DrawEllipse($pen, 4, 4, 24, 24)

# Draw "OH" text in the center
$font = New-Object System.Drawing.Font("Arial", 12, [System.Drawing.FontStyle]::Bold)
$brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$g.DrawString("OH", $font, $brush, 6, 8)

# Save as icon using FileStream
$icon = [System.Drawing.Icon]::FromHandle($bmp.GetHicon())
$fs = New-Object System.IO.FileStream("icons\icon.ico", [System.IO.FileMode]::Create)
$icon.Save($fs)
$fs.Close()

# Cleanup
$bmp.Dispose()
$g.Dispose()
$pen.Dispose()
$brush.Dispose()
$font.Dispose()
$icon.Dispose()

Write-Host "Created icons\icon.ico successfully!" -ForegroundColor Green
