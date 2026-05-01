# ==============================================================================
# Office Hub - 1-Click Local AI Setup
# Installs Ollama natively on Windows and pulls the Qwen 2.5 (0.5B) model
# for fast, offline, and lightweight AI capabilities.
# ==============================================================================

$ErrorActionPreference = "Stop"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "   Office Hub - Local AI Initialisation" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Check if Ollama is already installed
$ollamaExists = Get-Command "ollama" -ErrorAction SilentlyContinue

if (-not $ollamaExists) {
    Write-Host "[*] Ollama is not installed. Downloading installer..." -ForegroundColor Yellow
    
    $installerPath = Join-Path $env:TEMP "OllamaSetup.exe"
    $downloadUrl = "https://ollama.com/download/OllamaSetup.exe"

    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $installerPath -UseBasicParsing
        Write-Host "[*] Download complete. Installing Ollama..." -ForegroundColor Green
        
        # Start installer silently
        Start-Process -FilePath $installerPath -ArgumentList "/SILENT" -Wait -NoNewWindow
        
        Write-Host "[+] Ollama installed successfully!" -ForegroundColor Green
    } catch {
        Write-Host "[!] Failed to install Ollama. Please install it manually from https://ollama.com" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[+] Ollama is already installed." -ForegroundColor Green
}

# 2. Ensure Ollama service is running
Write-Host "[*] Starting Ollama service..." -ForegroundColor Yellow
Start-Process "ollama" -ArgumentList "serve" -WindowStyle Hidden -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3 # Give it a moment to start

# 3. Pull the Qwen 0.5B model (Lightweight for Office Hub)
$modelName = "qwen2.5:0.5b"
Write-Host "[*] Downloading AI Model ($modelName)... This may take a few minutes." -ForegroundColor Yellow

try {
    # Run ollama pull and wait for it
    $process = Start-Process "ollama" -ArgumentList "pull $modelName" -Wait -NoNewWindow -PassThru
    
    if ($process.ExitCode -eq 0) {
        Write-Host "[+] Model $modelName downloaded successfully!" -ForegroundColor Green
        Write-Host "[+] Local AI is ready to use in Office Hub." -ForegroundColor Cyan
    } else {
        Write-Host "[!] Failed to pull the model." -ForegroundColor Red
        exit 1
    }
} catch {
    Write-Host "[!] An error occurred while downloading the model." -ForegroundColor Red
    exit 1
}

exit 0
