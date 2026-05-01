@echo off
REM Session 3 Build Script for Office Hub
REM This script runs cargo check and npm build to verify the project compiles

echo ============================================
echo Office Hub - Session 3 Build Script
echo ============================================
echo.

REM Step 1: Run cargo check for Rust backend
echo [Step 1/3] Running cargo check for Rust backend...
echo This may take 5-10 minutes for the first build...
echo.

cd /d "%~dp0src-tauri"
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to change to src-tauri directory
    exit /b 1
)

cargo check
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ============================================
    echo ERROR: cargo check failed!
    echo Please review the errors above and fix them.
    echo ============================================
    exit /b 1
)

echo.
echo [SUCCESS] cargo check passed!
echo.

REM Step 2: Run npm install and npm build for frontend
echo [Step 2/3] Building frontend...
echo.

cd /d "%~dp0"
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to change to project root directory
    exit /b 1
)

echo Installing npm dependencies...
call npm install
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ============================================
    echo ERROR: npm install failed!
    echo Please review the errors above and fix them.
    echo ============================================
    exit /b 1
)

echo.
echo Building frontend with npm run build...
call npm run build
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ============================================
    echo ERROR: npm run build failed!
    echo Please review the errors above and fix them.
    echo ============================================
    exit /b 1
)

echo.
echo [SUCCESS] Frontend build passed!
echo.

REM Step 3: Summary
echo ============================================
echo [SUCCESS] All builds passed!
echo ============================================
echo.
echo Next steps:
echo 1. Run 'npm run tauri dev' to start the development server
echo 2. Begin Phase 1 implementation tasks
echo.
echo Phase 1 Tasks:
echo - LLM Gateway: Implement real Gemini API call
echo - System Tray: Implement icon and context menu
echo - Frontend: Build basic chat interface
echo ============================================
echo.

exit /b 0
