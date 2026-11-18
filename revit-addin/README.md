# ManageBIM exporter for Revit

End users: run `ManageBIM-Revit-AddIn.exe` (in the ManageBIM `AddIns` folder) - per-user install, no admin.

Build from source (Revit does not need to be installed, reference assemblies come from NuGet):

    dotnet build -c Release -p:RevitVersion=2023 -p:NoDeploy=true   # net48, Revit 2023
    dotnet build -c Release -p:RevitVersion=2024 -p:NoDeploy=true   # net48, Revit 2024
    dotnet build -c Release -p:RevitVersion=2025 -p:NoDeploy=true   # net8, Revit 2025
    dotnet build -c Release -p:RevitVersion=2026 -p:NoDeploy=true   # net8, Revit 2026 (+2027)

Copy `bin/<version>/ManageBIM.RevitExporter.dll` to `installers/payload/revit/ManageBIM.RevitExporter.<version>.dll`
and rebuild the installer (`cargo build --release --bin ManageBIM-Revit-AddIn`).
On Windows, without `NoDeploy`, the build also installs the add-in for the current user.
