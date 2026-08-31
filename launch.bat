@echo off
setlocal

cd /d "%~dp0"

set "APP_PYTHON=%~dp0.venv_win\Scripts\python.exe"

if not exist "%APP_PYTHON%" (
    echo ERROR: The Python virtual environment was not found.
    echo Expected: %APP_PYTHON%
    echo.
    echo Create it and install the dependencies with:
    echo   python -m venv .venv_win
    echo   .venv_win\Scripts\python.exe -m pip install -r requirements.txt
    echo.
    pause
    exit /b 1
)

echo Starting PossessApp...
echo Open http://localhost:8000 in your browser.
echo Press Ctrl+C to stop the server.
echo.

"%APP_PYTHON%" "%~dp0app.py"

if errorlevel 1 (
    echo.
    echo PossessApp stopped with an error.
    pause
)

endlocal
