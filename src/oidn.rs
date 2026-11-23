//! Intel Open Image Denoise (Apache 2.0), incarcat la rulare din `<exe>\oidn\OpenImageDenoise.dll`.
//! Fara DLL aplicatia merge normal; doar optiunea Denoise din Render e indisponibila.
//! Folosim filtrul „RT” pe CPU cu imaginile auxiliare albedo + normala (rezultat mult mai curat).

use std::path::PathBuf;

/// Folderele in care se cauta `OpenImageDenoise.dll`, in ordine.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(d) = std::env::var_os("MANAGEBIM_OIDN_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        v.push(dir.join("oidn"));
        // testele ruleaza din target\<profil>\deps
        if let Some(up) = dir.parent() {
            v.push(up.join("oidn"));
        }
    }
    // in dezvoltare: third_party\oidn (tools\get-oidn.ps1)
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("third_party").join("oidn"));
    v
}

/// Imaginile de intrare: culoarea liniara (HDR) si, optional, albedo [0,1] si normala [-1,1]; toate RGB f32.
pub struct Input<'a> {
    pub width: usize,
    pub height: usize,
    pub color: &'a [f32],
    pub albedo: Option<&'a [f32]>,
    pub normal: Option<&'a [f32]>,
}

#[cfg(windows)]
mod ffi {
    use std::ffi::{c_char, c_void, CStr};
    use std::sync::OnceLock;

    type Handle = *mut c_void;

    #[allow(clippy::type_complexity)]
    pub struct Api {
        pub dir: String,
        new_device: unsafe extern "C" fn(i32) -> Handle,
        commit_device: unsafe extern "C" fn(Handle),
        release_device: unsafe extern "C" fn(Handle),
        device_error: unsafe extern "C" fn(Handle, *mut *const c_char) -> i32,
        new_filter: unsafe extern "C" fn(Handle, *const c_char) -> Handle,
        set_image: unsafe extern "C" fn(Handle, *const c_char, *mut c_void, i32, usize, usize, usize, usize, usize),
        set_bool: unsafe extern "C" fn(Handle, *const c_char, bool),
        commit_filter: unsafe extern "C" fn(Handle),
        execute_filter: unsafe extern "C" fn(Handle),
        release_filter: unsafe extern "C" fn(Handle),
    }

    // pointerii la functii sunt doar adrese de cod din DLL
    unsafe impl Send for Api {}
    unsafe impl Sync for Api {}

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x0000_0100;
    const LOAD_LIBRARY_SEARCH_DEFAULT_DIRS: u32 = 0x0000_1000;
    const DEVICE_CPU: i32 = 1;
    const FORMAT_FLOAT3: i32 = 3;

    static API: OnceLock<Result<Api, String>> = OnceLock::new();

    pub fn api() -> Result<&'static Api, String> {
        API.get_or_init(load).as_ref().map_err(|e| e.clone())
    }

    fn load() -> Result<Api, String> {
        let Some(dll) = super::search_dirs().into_iter().map(|d| d.join("OpenImageDenoise.dll")).find(|p| p.is_file()) else {
            return Err("Intel Open Image Denoise (oidn\\OpenImageDenoise.dll) is not installed next to ManageBIM.exe.".into());
        };
        let dll = std::fs::canonicalize(&dll).unwrap_or(dll);
        let shown = dll.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        let wide: Vec<u16> = shown.encode_utf16().chain(std::iter::once(0)).collect();
        // OpenImageDenoise_core.dll, _device_cpu.dll si tbb12.dll se cauta in acelasi folder
        let h = unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS) };
        if h.is_null() {
            let code = unsafe { GetLastError() };
            let why = match code {
                126 => "a dependent DLL is missing".to_string(),
                193 => "wrong architecture (needs 64-bit)".to_string(),
                1260 | 4551 | 4556 => "blocked by Windows (Smart App Control / policy)".to_string(),
                c => format!("Windows error {c}"),
            };
            return Err(format!("Cannot load {shown}: {why}."));
        }
        macro_rules! sym {
            ($n:literal) => {{
                let p = unsafe { GetProcAddress(h, concat!($n, "\0").as_ptr() as *const c_char) };
                if p.is_null() {
                    return Err(format!("{shown} is not a supported version (missing {}).", $n));
                }
                unsafe { std::mem::transmute::<*mut c_void, _>(p) }
            }};
        }
        let dir = dll.parent().map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string()).unwrap_or_default();
        Ok(Api {
            dir,
            new_device: sym!("oidnNewDevice"),
            commit_device: sym!("oidnCommitDevice"),
            release_device: sym!("oidnReleaseDevice"),
            device_error: sym!("oidnGetDeviceError"),
            new_filter: sym!("oidnNewFilter"),
            set_image: sym!("oidnSetSharedFilterImage"),
            set_bool: sym!("oidnSetFilterBool"),
            commit_filter: sym!("oidnCommitFilter"),
            execute_filter: sym!("oidnExecuteFilter"),
            release_filter: sym!("oidnReleaseFilter"),
        })
    }

    impl Api {
        fn check(&self, dev: Handle) -> Result<(), String> {
            let mut msg: *const c_char = std::ptr::null();
            let code = unsafe { (self.device_error)(dev, &mut msg) };
            if code == 0 {
                return Ok(());
            }
            let m = if msg.is_null() { String::new() } else { unsafe { CStr::from_ptr(msg) }.to_string_lossy().into_owned() };
            Err(format!("Open Image Denoise error {code}: {m}"))
        }

        pub fn denoise(&self, inp: &super::Input) -> Result<Vec<f32>, String> {
            let (w, h) = (inp.width, inp.height);
            let n = w * h * 3;
            if w == 0 || h == 0 || inp.color.len() < n || inp.albedo.map_or(false, |a| a.len() < n) || inp.normal.map_or(false, |a| a.len() < n) {
                return Err("Denoise: image buffers have the wrong size.".into());
            }
            let mut out = vec![0.0f32; n];
            unsafe {
                let dev = (self.new_device)(DEVICE_CPU);
                if dev.is_null() {
                    return Err("Open Image Denoise: cannot create the CPU device.".into());
                }
                (self.commit_device)(dev);
                let result = (|| {
                    self.check(dev)?;
                    let f = (self.new_filter)(dev, c"RT".as_ptr());
                    if f.is_null() {
                        self.check(dev)?;
                        return Err("Open Image Denoise: cannot create the RT filter.".to_string());
                    }
                    // OIDN nu scrie in intrari; API-ul C cere totusi *mut
                    (self.set_image)(f, c"color".as_ptr(), inp.color.as_ptr() as *mut c_void, FORMAT_FLOAT3, w, h, 0, 0, 0);
                    if let Some(a) = inp.albedo {
                        (self.set_image)(f, c"albedo".as_ptr(), a.as_ptr() as *mut c_void, FORMAT_FLOAT3, w, h, 0, 0, 0);
                        // normala ajuta doar impreuna cu albedo
                        if let Some(nm) = inp.normal {
                            (self.set_image)(f, c"normal".as_ptr(), nm.as_ptr() as *mut c_void, FORMAT_FLOAT3, w, h, 0, 0, 0);
                        }
                    }
                    (self.set_image)(f, c"output".as_ptr(), out.as_mut_ptr() as *mut c_void, FORMAT_FLOAT3, w, h, 0, 0, 0);
                    (self.set_bool)(f, c"hdr".as_ptr(), true);
                    (self.commit_filter)(f);
                    let r = self.check(dev).and_then(|_| {
                        (self.execute_filter)(f);
                        self.check(dev)
                    });
                    (self.release_filter)(f);
                    r
                })();
                (self.release_device)(dev);
                result?;
            }
            Ok(out)
        }
    }
}

/// OIDN e disponibil? (Ok = folderul DLL-ului)
pub fn available() -> Result<String, String> {
    #[cfg(windows)]
    {
        ffi::api().map(|a| a.dir.clone())
    }
    #[cfg(not(windows))]
    {
        Err("Open Image Denoise is only bundled with the Windows version.".into())
    }
}

/// Curata zgomotul unei imagini randate (RGB liniar). Intoarce imaginea curatata, aceeasi marime.
pub fn denoise(inp: &Input) -> Result<Vec<f32>, String> {
    #[cfg(windows)]
    {
        ffi::api()?.denoise(inp)
    }
    #[cfg(not(windows))]
    {
        let _ = inp;
        Err("Open Image Denoise is only bundled with the Windows version.".into())
    }
}

#[cfg(test)]
mod tests {
    /// Cu DLL-ul descarcat (tools\get-oidn.ps1): o imagine cu zgomot devine mult mai neteda.
    #[test]
    fn denoises_noisy_image_when_available() {
        if super::available().is_err() {
            eprintln!("OIDN not installed - test skipped");
            return;
        }
        let (w, h) = (64usize, 64usize);
        let mut s = 12345u32;
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32) - 0.5
        };
        let color: Vec<f32> = (0..w * h * 3).map(|_| 0.5 + 0.4 * rnd()).collect();
        let albedo = vec![0.5f32; w * h * 3];
        let normal: Vec<f32> = (0..w * h).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
        let out = super::denoise(&super::Input { width: w, height: h, color: &color, albedo: Some(&albedo), normal: Some(&normal) }).expect("denoise");
        let var = |v: &[f32]| {
            let m = v.iter().sum::<f32>() / v.len() as f32;
            v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32
        };
        assert_eq!(out.len(), color.len());
        assert!(out.iter().all(|x| x.is_finite()));
        assert!(var(&out) < var(&color) * 0.25, "noise not reduced: {} vs {}", var(&out), var(&color));
    }
}
