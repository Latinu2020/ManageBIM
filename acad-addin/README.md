# ManageBIM exporter for AutoCAD

End users: run `ManageBIM-AutoCAD-AddIn.exe` - installs `%APPDATA%\Autodesk\ApplicationPlugins\ManageBIM.bundle` (per user, no admin).
Command `BIMCEXPORT`, ribbon tab "ManageBIM".

    dotnet build -c Release -p:AcadSeries=R24   # AutoCAD 2021-2024 (.NET Framework 4.8)
    dotnet build -c Release -p:AcadSeries=R25   # AutoCAD 2025+     (.NET 8)

Copy `bin/R24|R25/ManageBIM.AcadExporter.dll` to `installers/payload/acad/ManageBIM.AcadExporter.R24|R25.dll`,
then `cargo build --release --bin ManageBIM-AutoCAD-AddIn`.
