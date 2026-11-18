@echo off
rem Instaleaza add-in-ul ManageBIM in Navisworks (utilizatorul curent). "install.cmd /u" il dezinstaleaza.
if /i "%~1"=="/u" (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" -Uninstall
) else (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
)
pause
