@echo off
rem Builds and installs the ManageBIM exporter for the given Revit version (default 2024).
rem Requires the .NET 8 SDK: https://dotnet.microsoft.com/download
set VER=%1
if "%VER%"=="" set VER=2024
dotnet build "%~dp0ManageBIM.RevitExporter.csproj" -c Release -p:RevitVersion=%VER%
if errorlevel 1 (echo Build failed & pause & exit /b 1)
echo.
echo Installed. Restart Revit %VER% - you will find the "ManageBIM" tab on the ribbon.
pause
