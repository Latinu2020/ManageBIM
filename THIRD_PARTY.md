# Componente terțe în ManageBIM

- **Open CASCADE Technology 8.0.1** - LGPL-2.1 + Open CASCADE exception, legat dinamic (folderul `occt\`, vezi `occt\NOTICE.txt`).
- **Mesa 3D 26.3.1** (software OpenGL - llvmpipe - for PCs without OpenGL 3.3, Settings › Software rendering) - MIT; Windows build by mesa-dist-win (github.com/pal1000/mesa-dist-win, MIT); includes LLVM (Apache-2.0 with LLVM exception). Dynamically loaded from the `mesa\` folder (with `LICENSE.txt`); downloaded by `tools/get-mesa.ps1` (SHA-256 checked).
- **Microsoft Visual C++ runtime** (vcruntime140, msvcp140, concrt140) - redistributable files of Visual Studio, shipped next to ManageBIM.exe for Open Image Denoise / oneTBB, under the Visual Studio redistribution terms.
- **Intel Open Image Denoise 2.5.1** (Render › Denoise) - Apache-2.0, legat dinamic (folderul `oidn\`, cu `LICENSE.txt` și `third-party-programs*.txt`). Include **oneTBB** (Apache-2.0). Descărcat de `tools/get-oidn.ps1` de pe github.com/RenderKit/oidn.
- **Inter 4.1** (fontul interfeței, github.com/rsms/inter) - SIL Open Font License 1.1, inclus în executabil; textul licenței: `assets/fonts/Inter-LICENSE.txt`.
- **Lato 2.0** (al doilea font inclus, alegibil din Settings › Font; latofonts.com, din google/fonts) - SIL Open Font License 1.1, inclus în executabil; textul licenței: `assets/fonts/LATO-LICENSE.txt`.
- **Phosphor Icons** (iconițele vectoriale, prin crate-ul `egui-phosphor` 0.7) - MIT.
- **ufbx 0.23.1** (citire FBX) - MIT, legat static. Textul licenței:

```text
------------------------------------------------------------------------------
This software is available under 2 licenses -- choose whichever you prefer.
------------------------------------------------------------------------------
ALTERNATIVE A - MIT License
Copyright (c) 2020 Samuli Raivio
Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
------------------------------------------------------------------------------
ALTERNATIVE B - Public Domain (www.unlicense.org)
This is free and unencumbered software released into the public domain.
Anyone is free to copy, modify, publish, use, compile, sell, or distribute this
software, either in source code form or as a compiled binary, for any purpose,
commercial or non-commercial, and by any means.
In jurisdictions that recognize copyright laws, the author or authors of this
software dedicate any and all copyright interest in the software to the public
domain. We make this dedication for the benefit of the public at large and to
the detriment of our heirs and successors. We intend this dedication to be an
overt act of relinquishment in perpetuity of all present and future rights to
this software under copyright law.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
----------------------------------------

```

- **SQLite** (prin rusqlite, citirea bazei de date Plant 3D) - domeniu public / MIT.
- **acadrust** (citire DWG) - MPL-2.0.
- **smartstring** (used by the Rhai scripting engine) - MPL-2.0; unmodified, source at crates.io/crates/smartstring.
- **laz-rs** (crate-ul `laz`, decompresia nori LAZ / LASzip) - Apache-2.0 / MIT.
- **Schemele IFC EXPRESS** (IFC2X3_TC1, IFC4 ADD2 TC1, IFC4X3) - © buildingSMART International Limited; fișierele oficiale, nemodificate, în `third_party/ifc-schema/` (preluate din github.com/ThatOpen/engine_web-ifc). buildingSMART permite copierea și distribuirea documentației tehnice nemodificate (vezi antetul fiecărui fișier). Folosite pentru ifcXML / ifcJSON -> IFC STEP.
- Celelalte biblioteci Rust (egui, glam, rayon...) - MIT / Apache-2.0, lista completă în `Cargo.lock`.

## ambientCG materials (default PBR library)

ManageBIM ships with PBR texture sets from
[ambientCG](https://ambientcg.com) (Lennart Demes). They are licensed **CC0 1.0 Universal**
(public domain dedication): free for personal and commercial use, no attribution required.
They are bundled in the installer (`<app>\materials\`, from `third_party\pbr-library`); a 2K version can be downloaded from Render › Materials.
