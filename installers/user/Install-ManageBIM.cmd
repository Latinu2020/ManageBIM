@echo off
rem Instaleaza ManageBIM pentru utilizatorul curent (fara administrator): aplicatia, scurtaturi, add-in-uri.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
pause
