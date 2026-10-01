@echo off
REM Compila BoquilaHUB en Windows (modo debug). Uso: doble clic o `compilar.bat` desde una terminal.
REM Deja todo el detalle en build_log.txt
cd /d "%~dp0"

where cargo >nul 2>&1 || (echo [ERROR] No se encontro cargo. Instala Rust desde https://rustup.rs & pause & exit /b 1)

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" (echo [ERROR] No se encontro Visual Studio. Instala Visual Studio con "Desarrollo para el escritorio con C++". & pause & exit /b 1)
set "VSPATH="
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSPATH=%%i"
if not defined VSPATH (echo [ERROR] Visual Studio no tiene las herramientas de C++. Agrega "Desarrollo para el escritorio con C++" desde Visual Studio Installer. & pause & exit /b 1)
echo Usando Visual Studio en: %VSPATH%

echo ===== inicio ===== > build_log.txt
echo VS: %VSPATH% >> build_log.txt

if not exist "deps\ffmpeg" (
  echo Descargando ffmpeg y ONNX Runtime ^(solo la primera vez^)...
  cargo xtask fetch >> build_log.txt 2>&1
)

echo Compilando... la primera vez puede tardar 10+ minutos.
cmd /c ""%VSPATH%\VC\Auxiliary\Build\vcvars64.bat" >nul && cargo build" >> build_log.txt 2>&1
set "RC=%ERRORLEVEL%"
echo EXITCODE=%RC% >> build_log.txt

echo.
if "%RC%"=="0" (
  echo [OK] Compilado: target\debug\boquilahub.exe
  echo      Para abrir la interfaz:  cargo run -- gui
) else (
  echo [ERROR] Fallo la compilacion. Revisa build_log.txt
)
pause
