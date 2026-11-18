//! Frame sinks for the Render tab: MJPEG AVI (plays in Windows / PowerPoint / VLC),
//! MP4 (through ffmpeg when installed), animated GIF and PNG sequences; PNG still images.
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum VideoFormat {
    Avi,
    Mp4,
    Gif,
    PngSequence,
}

impl VideoFormat {
    pub const ALL: [VideoFormat; 4] = [VideoFormat::Mp4, VideoFormat::Avi, VideoFormat::Gif, VideoFormat::PngSequence];
    pub fn label(&self) -> &'static str {
        match self {
            VideoFormat::Avi => "AVI (Motion JPEG) - Windows, PowerPoint, VLC",
            VideoFormat::Mp4 => "MP4 (H.264) - needs ffmpeg",
            VideoFormat::Gif => "Animated GIF (max 960 px wide)",
            VideoFormat::PngSequence => "PNG image sequence",
        }
    }
    pub fn ext(&self) -> &'static str {
        match self {
            VideoFormat::Avi => "avi",
            VideoFormat::Mp4 => "mp4",
            VideoFormat::Gif => "gif",
            VideoFormat::PngSequence => "png",
        }
    }
}

/// RGBA8 frame, top row first.
pub struct Frame {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn rgb(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity((self.w * self.h * 3) as usize);
        for p in self.rgba.chunks_exact(4) {
            v.extend_from_slice(&p[..3]);
        }
        v
    }
    /// Box-filter downscale by an integer factor (supersampling).
    pub fn downscale(&self, f: u32) -> Frame {
        if f <= 1 {
            return Frame { w: self.w, h: self.h, rgba: self.rgba.clone() };
        }
        let (w, h) = (self.w / f, self.h / f);
        let mut out = vec![0u8; (w * h * 4) as usize];
        let n = f * f;
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0u32; 4];
                for dy in 0..f {
                    for dx in 0..f {
                        let i = (((y * f + dy) * self.w + x * f + dx) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += self.rgba[i + c] as u32;
                        }
                    }
                }
                let o = ((y * w + x) * 4) as usize;
                for c in 0..4 {
                    out[o + c] = (acc[c] / n) as u8;
                }
            }
        }
        Frame { w, h, rgba: out }
    }
    /// Nearest-neighbour resize to a maximum width (GIF).
    pub fn fit_width(&self, max_w: u32) -> Frame {
        if self.w <= max_w {
            return Frame { w: self.w, h: self.h, rgba: self.rgba.clone() };
        }
        let w = max_w;
        let h = ((self.h as u64 * max_w as u64) / self.w as u64).max(1) as u32;
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let sy = (y as u64 * self.h as u64 / h as u64) as u32;
            for x in 0..w {
                let sx = (x as u64 * self.w as u64 / w as u64) as u32;
                let i = ((sy * self.w + sx) * 4) as usize;
                let o = ((y * w + x) * 4) as usize;
                out[o..o + 4].copy_from_slice(&self.rgba[i..i + 4]);
            }
        }
        Frame { w, h, rgba: out }
    }
}

pub fn save_png(path: &Path, f: &Frame) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), f.w, f.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&f.rgb()).map_err(|e| e.to_string())?;
    Ok(())
}

// ------------------------------------------------------------------ AVI (MJPEG)
pub struct AviWriter {
    f: BufWriter<File>,
    w: u32,
    h: u32,
    fps: u32,
    quality: u8,
    movi_fourcc_pos: u64,
    index: Vec<(u32, u32)>,
    max_chunk: u32,
}

fn fourcc(s: &[u8; 4]) -> [u8; 4] {
    *s
}

impl AviWriter {
    pub fn create(path: &Path, w: u32, h: u32, fps: u32, quality: u8) -> Result<AviWriter, String> {
        let file = File::create(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
        let mut a = AviWriter { f: BufWriter::new(file), w, h, fps: fps.max(1), quality, movi_fourcc_pos: 0, index: Vec::new(), max_chunk: 0 };
        a.write_headers(0).map_err(|e| e.to_string())?;
        Ok(a)
    }

    fn u32(&mut self, v: u32) -> std::io::Result<()> {
        self.f.write_all(&v.to_le_bytes())
    }
    fn u16(&mut self, v: u16) -> std::io::Result<()> {
        self.f.write_all(&v.to_le_bytes())
    }

    /// Fixed-size header (rewritten at the end with the real counts).
    fn write_headers(&mut self, frames: u32) -> std::io::Result<()> {
        let (w, h, fps) = (self.w, self.h, self.fps);
        self.f.write_all(&fourcc(b"RIFF"))?;
        self.u32(0)?; // patched
        self.f.write_all(b"AVI ")?;
        // hdrl: 4 + (8+56) + (8 + 4 + (8+56) + (8+40)) = 192
        self.f.write_all(b"LIST")?;
        self.u32(192)?;
        self.f.write_all(b"hdrl")?;
        self.f.write_all(b"avih")?;
        self.u32(56)?;
        self.u32(1_000_000 / fps)?;
        self.u32(self.max_chunk.saturating_mul(fps))?;
        self.u32(0)?;
        self.u32(0x10)?; // AVIF_HASINDEX
        self.u32(frames)?;
        self.u32(0)?;
        self.u32(1)?;
        self.u32(self.max_chunk.max(w * h))?;
        self.u32(w)?;
        self.u32(h)?;
        for _ in 0..4 {
            self.u32(0)?;
        }
        self.f.write_all(b"LIST")?;
        self.u32(4 + 64 + 48)?;
        self.f.write_all(b"strl")?;
        self.f.write_all(b"strh")?;
        self.u32(56)?;
        self.f.write_all(b"vids")?;
        self.f.write_all(b"MJPG")?;
        self.u32(0)?;
        self.u16(0)?;
        self.u16(0)?;
        self.u32(0)?;
        self.u32(1)?; // scale
        self.u32(fps)?; // rate
        self.u32(0)?;
        self.u32(frames)?;
        self.u32(self.max_chunk.max(w * h))?;
        self.u32(0xFFFF_FFFF)?;
        self.u32(0)?;
        self.u16(0)?;
        self.u16(0)?;
        self.u16(w as u16)?;
        self.u16(h as u16)?;
        self.f.write_all(b"strf")?;
        self.u32(40)?;
        self.u32(40)?;
        self.u32(w)?;
        self.u32(h)?;
        self.u16(1)?;
        self.u16(24)?;
        self.f.write_all(b"MJPG")?;
        self.u32(w * h * 3)?;
        for _ in 0..4 {
            self.u32(0)?;
        }
        self.f.write_all(b"LIST")?;
        self.u32(0)?; // patched
        self.movi_fourcc_pos = self.f.stream_position()?;
        self.f.write_all(b"movi")?;
        Ok(())
    }

    pub fn add(&mut self, frame: &Frame) -> Result<(), String> {
        let mut jpg = Vec::with_capacity((frame.w * frame.h) as usize / 4);
        let enc = jpeg_encoder::Encoder::new(&mut jpg, self.quality);
        enc.encode(&frame.rgb(), frame.w as u16, frame.h as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
        let pos = self.f.stream_position().map_err(|e| e.to_string())?;
        let off = (pos - self.movi_fourcc_pos) as u32;
        (|| -> std::io::Result<()> {
            self.f.write_all(b"00dc")?;
            self.u32(jpg.len() as u32)?;
            self.f.write_all(&jpg)?;
            if jpg.len() % 2 == 1 {
                self.f.write_all(&[0])?;
            }
            Ok(())
        })()
        .map_err(|e| e.to_string())?;
        self.max_chunk = self.max_chunk.max(jpg.len() as u32);
        self.index.push((off, jpg.len() as u32));
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), String> {
        (|| -> std::io::Result<()> {
            let movi_end = self.f.stream_position()?;
            self.f.write_all(b"idx1")?;
            self.u32(self.index.len() as u32 * 16)?;
            for (off, len) in self.index.clone() {
                self.f.write_all(b"00dc")?;
                self.u32(0x10)?;
                self.u32(off)?;
                self.u32(len)?;
            }
            let end = self.f.stream_position()?;
            // patch RIFF + movi sizes and the headers
            let frames = self.index.len() as u32;
            self.f.seek(SeekFrom::Start(0))?;
            self.write_headers(frames)?;
            self.f.seek(SeekFrom::Start(4))?;
            self.u32((end - 8) as u32)?;
            self.f.seek(SeekFrom::Start(self.movi_fourcc_pos - 4))?;
            self.u32((movi_end - self.movi_fourcc_pos) as u32)?;
            self.f.seek(SeekFrom::Start(end))?;
            self.f.flush()
        })()
        .map_err(|e| e.to_string())
    }
}

// ------------------------------------------------------------------ generic sink
pub enum Sink {
    Avi(AviWriter),
    Gif(gif::Encoder<BufWriter<File>>, u16),
    Png(PathBuf, u32),
}

/// Starts a writer for `path`. For MP4 an intermediate AVI is written and converted at the end.
pub fn open_sink(format: VideoFormat, path: &Path, w: u32, h: u32, fps: u32, quality: u8) -> Result<Sink, String> {
    match format {
        VideoFormat::Avi => Ok(Sink::Avi(AviWriter::create(path, w, h, fps, quality)?)),
        VideoFormat::Mp4 => Ok(Sink::Avi(AviWriter::create(&mp4_temp(path), w, h, fps, 95)?)),
        VideoFormat::Gif => {
            let f = Frame { w, h, rgba: Vec::new() };
            let gw = w.min(960);
            let gh = if w > 960 { ((f.h as u64 * 960) / f.w as u64) as u32 } else { h };
            let file = File::create(path).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
            let mut enc = gif::Encoder::new(BufWriter::new(file), gw as u16, gh as u16, &[]).map_err(|e| e.to_string())?;
            enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
            Ok(Sink::Gif(enc, (100 / fps.max(1)).max(2) as u16))
        }
        VideoFormat::PngSequence => {
            let dir = png_dir(path);
            std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
            Ok(Sink::Png(dir, 0))
        }
    }
}

pub fn mp4_temp(path: &Path) -> PathBuf {
    path.with_extension("tmp.avi")
}

pub fn png_dir(path: &Path) -> PathBuf {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "frames".into());
    path.parent().unwrap_or(Path::new(".")).join(format!("{stem}_frames"))
}

impl Sink {
    pub fn add(&mut self, f: &Frame) -> Result<(), String> {
        match self {
            Sink::Avi(a) => a.add(f),
            Sink::Gif(enc, delay) => {
                let mut s = f.fit_width(960);
                let mut fr = gif::Frame::from_rgba_speed(s.w as u16, s.h as u16, &mut s.rgba, 10);
                fr.delay = *delay;
                enc.write_frame(&fr).map_err(|e| e.to_string())
            }
            Sink::Png(dir, n) => {
                *n += 1;
                save_png(&dir.join(format!("frame_{:05}.png", *n)), f)
            }
        }
    }
    pub fn finish(self) -> Result<(), String> {
        match self {
            Sink::Avi(a) => a.finish(),
            Sink::Gif(enc, _) => {
                drop(enc);
                Ok(())
            }
            Sink::Png(..) => Ok(()),
        }
    }
}

fn ffmpeg_cmd() -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

pub fn ffmpeg_available() -> bool {
    ffmpeg_cmd().arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Converts the intermediate AVI to H.264 MP4 with ffmpeg.
pub fn convert_mp4(path: &Path) -> Result<(), String> {
    let tmp = mp4_temp(path);
    let st = ffmpeg_cmd()
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&tmp)
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(path)
        .status()
        .map_err(|e| format!("ffmpeg could not be started: {e}"))?;
    if st.success() {
        let _ = std::fs::remove_file(&tmp);
        Ok(())
    } else {
        Err(format!("ffmpeg failed - the AVI was kept: {}", tmp.display()))
    }
}
