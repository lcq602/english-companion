@echo off
setlocal EnableExtensions

cd /d "%~dp0"

rem English Companion 本地打包脚本（Windows）。
rem 若本机按旧约定把 Rust 放在 E:\Rust，则沿用；否则使用系统默认 Rust 安装。

if exist "E:\Rust\cargo\bin\cargo.exe" (
    set "RUSTUP_HOME=E:\Rust\rustup"
    set "CARGO_HOME=E:\Rust\cargo"
    set "CARGO_TARGET_DIR=E:\Rust\target"
    set "PATH=E:\Rust\cargo\bin;%PATH%"
    if exist "E:\software\visualStudio\VSBuildTools\Common7\Tools\VsDevCmd.bat" (
        call "E:\software\visualStudio\VSBuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64
    )
) else (
    set "CARGO_TARGET_DIR=%~dp0src-tauri\target"
)

where cargo >nul 2>&1
if errorlevel 1 (
    echo ERROR: cargo was not found. Install Rust stable first.
    pause
    exit /b 1
)

where node >nul 2>&1
if errorlevel 1 (
    echo ERROR: Node.js was not found. Install Node.js 20 or newer.
    pause
    exit /b 1
)

if not exist "node_modules" (
    echo Installing frontend dependencies...
    call npm.cmd ci
    if errorlevel 1 (
        echo ERROR: npm ci failed.
        pause
        exit /b 1
    )
)

echo Running tests...
call npm.cmd test
if errorlevel 1 ( echo ERROR: frontend tests failed. & pause & exit /b 1 )
call cargo test --manifest-path src-tauri\Cargo.toml
if errorlevel 1 ( echo ERROR: rust tests failed. & pause & exit /b 1 )

echo Building the NSIS installer and portable exe...
call npm.cmd run tauri build -- --bundles nsis
if errorlevel 1 (
    echo ERROR: Tauri build failed.
    pause
    exit /b 1
)

echo.
echo Build completed successfully.
echo Portable exe: %CARGO_TARGET_DIR%\release\english-companion.exe
echo Installer:    %CARGO_TARGET_DIR%\release\bundle\nsis\English Companion_1.0.0_x64-setup.exe
pause
exit /b 0
