use bimrust::markup::*;

fn item(i: usize) -> ReportItem {
    let rgb: Vec<u8> = (0..64 * 36).flat_map(|k| [(k % 64 * 4) as u8, 120, 200]).collect();
    ReportItem {
        index: i,
        name: format!("Clash at grid B-{i} <pipe & beam>"),
        description: "Duct crosses the beam\nmove 200 mm up".into(),
        issue: Issue::default(),
        comments: vec![VpComment { author: "reviewer".into(), date: now_iso(), text: "Checked on site".into() }],
        redlines: 3,
        eye: [100.5, 200.25, 12.0],
        dir: [0.0, 1.0, 0.0],
        up: [0.0, 0.0, 1.0],
        fov_deg: 50.0,
        ortho_height: if i.is_multiple_of(2) { Some(20.0) } else { None },
        selected_guids: vec!["2O2Fr$t4X7Zf8NOew3FLOH".into()],
        hidden_guids: vec![],
        image: encode_png(&rgb, 64, 36),
        image_w: 64,
        image_h: 36,
    }
}

#[test]
fn reports_are_written() {
    let items: Vec<ReportItem> = (1..=3).map(item).collect();
    let dir = std::env::temp_dir().join("managebim_reports");
    std::fs::create_dir_all(&dir).unwrap();
    let html = html_report("Demo", &items);
    assert!(html.contains("data:image/png;base64,") && html.contains("&lt;pipe &amp; beam&gt;"));
    std::fs::write(dir.join("r.html"), html).unwrap();
    let thumbs: Vec<(Vec<u8>, u32, u32)> = items.iter().map(|i| (i.image.clone(), 64, 36)).collect();
    let x = xlsx_report("Demo", &items, &thumbs);
    assert_eq!(&x[0..2], b"PK");
    std::fs::write(dir.join("r.xlsx"), x).unwrap();
    let b = bcf_zip("Demo", &items);
    std::fs::write(dir.join("r.bcf"), b).unwrap();
    let g = new_guid();
    assert_eq!(g.len(), 36);
    assert_ne!(g, new_guid());
    assert!(now_iso().ends_with('Z'));
}
