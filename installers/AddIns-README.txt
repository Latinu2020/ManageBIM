ManageBIM add-ins - installers for your user only (no administrator rights needed)
=================================================================================

ManageBIM-Revit-AddIn.exe       Revit 2023 - 2027
    Ribbon tab "ManageBIM" > Export BIMC: the 3D view with geometry, colours, levels,
    all instance + type parameters, shared coordinates.

ManageBIM-AutoCAD-AddIn.exe     AutoCAD 2021+ and verticals (Plant 3D, Architecture, MEP,
                                Civil 3D, Map 3D, Advance Steel)
    Command BIMCEXPORT / ribbon tab "ManageBIM". In AutoCAD Plant 3D open the drawing
    from its project: the Plant 3D Data Manager properties (Tag, Size, Spec, Line Number...)
    are exported too. The first time AutoCAD asks whether to load ManageBIM: "Always Load".

ManageBIM-Navisworks-AddIn.exe  Navisworks Manage / Simulate 2022 - 2026
    Tool add-ins > Export to ManageBIM (.bimc). Each Navisworks version gets the add-in
    built for it. ManageBIM also uses it (with Navisworks\nwd2bimc.exe) to open
    .nwd / .nwc / .nwf / .rvt files directly.

Double-click an installer: the versions found on this PC are ticked -> Install.
Close Revit / AutoCAD / Navisworks first. "Uninstall" removes the add-in again.

Command line (e.g. for deployment scripts):
    ManageBIM-Revit-AddIn.exe --install      (also --uninstall, --list)
