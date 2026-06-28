@echo off
setlocal enabledelayedexpansion
echo ==============================================================
echo   UtenCore Windows Build
echo ==============================================================
echo.

set "VSROOT=G:\Programs\Microsoft\Microsoft Visual Studio\2022"
set "VCVARS="

if exist "%VSROOT%\Enterprise\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%VSROOT%\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
if exist "%VSROOT%\Professional\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%VSROOT%\Professional\VC\Auxiliary\Build\vcvars64.bat"
if exist "%VSROOT%\Community\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS=%VSROOT%\Community\VC\Auxiliary\Build\vcvars64.bat"

if "%VCVARS%"=="" (
    echo [ERROR] Visual Studio 2022 not found at %VSROOT%
    exit /b 1
)

echo [*] Activating MSVC...
call "%VCVARS%" >NUL
if %ERRORLEVEL% neq 0 exit /b 1

echo [*] Checking Rust...
cargo --version >NUL 2>&1 || (echo [ERROR] cargo not found & exit /b 1)
rustc --version

echo.
echo [*] Building UtenCore (release)...
cargo build --release --no-default-features -p uc-binaries
if %ERRORLEVEL% neq 0 exit /b 1

echo.
echo [*] Artifacts:
for %%f in (target\release\uc.exe target\release\ucc.exe target\release\utencore.exe target\release\ucdump.exe) do (
    if exist %%f echo   %%f
)

echo.
echo Done.
