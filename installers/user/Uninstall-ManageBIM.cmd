@echo off
rem Dezinstaleaza ManageBIM si add-in-urile (setarile tale din %APPDATA%\ManageBIM raman).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" -Uninstall
pause
