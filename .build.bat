@echo off
REM Maix-Agent v0.1.1 Release Build
REM Usage: .build.bat [test|release|clean]

setlocal

set RELEASE_DIR=release\v0.1.1

if "%1"=="clean" (
    echo Cleaning...
    cargo clean
    goto :done
)

if "%1"=="test" (
    echo Running tests...
    cargo test --workspace
    if errorlevel 1 (
        echo Tests failed!
        pause
        exit /b 1
    )
    echo Tests passed.
    goto :done
)

echo Building release...
cargo build --release
if errorlevel 1 (
    echo Build failed!
    pause
    exit /b 1
)

echo Cleaning debug files...
del /q target\release\*.pdb 2>nul
del /q target\release\*.d 2>nul

echo Copying files to %RELEASE_DIR%...
if not exist %RELEASE_DIR% mkdir %RELEASE_DIR%
copy /y target\release\maix.exe %RELEASE_DIR%\ >nul
copy /y target\release\maix-cli.exe %RELEASE_DIR%\ >nul
copy /y target\release\maix-tui.exe %RELEASE_DIR%\ >nul
copy /y target\release\maix-gateway.exe %RELEASE_DIR%\ >nul
copy /y README.md %RELEASE_DIR%\ >nul
copy /y README_zh-CN.md %RELEASE_DIR%\ >nul
copy /y config\default.toml %RELEASE_DIR%\ >nul
copy /y LICENSE %RELEASE_DIR%\ >nul
copy /y COPYRIGHT %RELEASE_DIR%\ >nul

echo Done. Files in %RELEASE_DIR%\
dir /b %RELEASE_DIR%\

:done
pause
