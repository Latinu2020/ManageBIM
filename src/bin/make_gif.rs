//! Documentation tool: a sequence of PNG captures -> animated GIF (README).
//! `make_gif <folder with frame_*.png> <output.gif> [width=880] [fps=10]`
//! The frames are downscaled (block average), quantized (NeuQuant) and loop forever.
use std::fs::File;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: make_gif <frames dir> <out.gif> [width=880] [fps=10] [step=1]");
        std::process::exit(2);
    }
    let width: u32 = a.get(3).and_then(|v| v.parse().ok()).unwrap_or(880);
    let fps: u32 = a.get(4).and_then(|v| v.parse().ok()).unwrap_or(10);
    // the 5th argument: one frame out of N is kept (smaller GIF)
    let step: usize = a.get(5).and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let mut files: Vec<_> = std::fs::read_dir(&a[1]).expect("frames dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
    files.sort();
    let files: Vec<_> = files.into_iter().step_by(step).collect();
    let mut enc: Option<gif::Encoder<File>> = None;
    let mut prev: Option<Vec<u8>> = None;
    let mut hold = 0u16;
    let delay = (100 / fps.max(1)) as u16;
    let mut pending: Option<gif::Frame<'static>> = None;
    for f in &files {
        let dec = png::Decoder::new(File::open(f).expect("png"));
        let mut r = dec.read_info().expect("png info");
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).expect("png frame");
        let (w, h) = (info.width, info.height);
        let ch = info.color_type.samples();
        let s = (w as f32 / width as f32).max(1.0);
        let (ow, oh) = ((w as f32 / s) as u32, (h as f32 / s) as u32);
        let mut rgba = vec![0u8; (ow * oh * 4) as usize];
        // downscaling: the average of the pixels in each block
        for y in 0..oh {
            for x in 0..ow {
                let (x0, x1) = ((x as f32 * s) as u32, (((x + 1) as f32 * s) as u32).min(w).max((x as f32 * s) as u32 + 1));
                let (y0, y1) = ((y as f32 * s) as u32, (((y + 1) as f32 * s) as u32).min(h).max((y as f32 * s) as u32 + 1));
                let mut acc = [0u32; 3];
                let mut n = 0;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let o = ((yy * w + xx) as usize) * ch;
                        for c in 0..3 {
                            acc[c] += buf[o + c] as u32;
                        }
                        n += 1;
                    }
                }
                let o = ((y * ow + x) * 4) as usize;
                for c in 0..3 {
                    rgba[o + c] = (acc[c] / n.max(1)) as u8;
                }
                rgba[o + 3] = 255;
            }
        }
        // identical frames: the duration of the previous one is extended (smaller GIF)
        if prev.as_ref() == Some(&rgba) {
            hold += delay;
            if let Some(p) = pending.as_mut() {
                p.delay = hold;
            }
            continue;
        }
        if enc.is_none() {
            let mut e = gif::Encoder::new(File::create(&a[2]).expect("out"), ow as u16, oh as u16, &[]).expect("gif");
            e.set_repeat(gif::Repeat::Infinite).ok();
            enc = Some(e);
        }
        if let (Some(p), Some(e)) = (pending.take(), enc.as_mut()) {
            e.write_frame(&p).expect("write");
        }
        let mut px = rgba.clone();
        let mut fr = gif::Frame::from_rgba_speed(ow as u16, oh as u16, &mut px, 10);
        fr.delay = delay;
        hold = delay;
        pending = Some(fr);
        prev = Some(rgba);
    }
    if let (Some(mut p), Some(e)) = (pending.take(), enc.as_mut()) {
        // the last frame stays a bit longer before looping
        p.delay = p.delay.max(150);
        e.write_frame(&p).expect("write");
    }
    println!("{} frames -> {}", files.len(), a[2]);
}
