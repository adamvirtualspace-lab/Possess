@echo off
setlocal

cd /d "%~dp0"

set "APP_EXE=%~dp0possess.exe"
if not exist "%APP_EXE%" set "APP_EXE=%~dp0target\release\possess.exe"

if not exist "%APP_EXE%" (
    echo ERROR: The PossessApp binary was not found.
    echo Expected: %~dp0possess.exe
    echo.
    echo Build it with:
    echo   cargo build --release --features embed
    echo.
    echo Or install Rust first from https://rustup.rs
    pause
    exit /b 1
)

echo Starting PossessApp...
echo Open http://localhost:8000 in your browser.
echo Press Ctrl+C to stop the server.
echo.

"%APP_EXE%"

if errorlevel 1 (
    echo.
    echo PossessApp stopped with an error.
    pause
)

endlocal
