// sprdump <exe> <outdir> [filtro]: lista sprites y fondos; con filtro, guarda PNG de los que coinciden
use std::{fs, path::Path};
fn s(p: &gm8exe::asset::PascalString) -> String { p.0.iter().map(|&b| b as char).collect() }
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut exe = fs::read(&a[1]).unwrap();
    let o = Path::new(&a[2]);
    fs::create_dir_all(o).unwrap();
    let filt = a.get(3).map(|x| x.to_lowercase());
    let g = gm8exe::reader::from_exe(&mut exe, None::<fn(&str)>, false, true).unwrap();
    let mut list = String::new();
    for (i, sp) in g.sprites.iter().enumerate() {
        let Some(sp) = sp else { continue };
        let n = s(&sp.name);
        let f0 = sp.frames.first();
        // estadística: píxeles alfa 0 / alfa 255 / intermedios y alfa 0 con RGB no negro
        let (mut a0, mut a255, mut amid, mut hid) = (0, 0, 0, 0);
        if let Some(f) = f0 {
            for p in f.data.chunks(4) {
                match p[3] { 0 => { a0 += 1; if p[0] | p[1] | p[2] != 0 { hid += 1 } }, 255 => a255 += 1, _ => amid += 1 }
            }
        }
        list.push_str(&format!("{}\t{}\t{}x{}\tframes={}\ta0={} a255={} amid={} oculto={}\n", i, n,
            f0.map_or(0, |f| f.width), f0.map_or(0, |f| f.height), sp.frames.len(), a0, a255, amid, hid));
        if let Some(fl) = &filt {
            if n.to_lowercase().contains(fl.as_str()) {
                for (k, f) in sp.frames.iter().enumerate().take(4) {
                    let mut rgba = f.data.to_vec();
                    for p in rgba.chunks_mut(4) { p.swap(0, 2) } // BGRA -> RGBA
                    image::save_buffer(o.join(format!("{}_{}_{}.png", i, n, k)), &rgba, f.width, f.height, image::ColorType::Rgba8).ok();
                }
            }
        }
    }
    fs::write(o.join("sprites.tsv"), list).unwrap();
    let mut bl = String::new();
    for (i, b) in g.backgrounds.iter().enumerate() {
        let Some(b) = b else { continue };
        bl.push_str(&format!("{}\t{}\t{}x{}\n", i, s(&b.name), b.width, b.height));
    }
    fs::write(o.join("backgrounds.tsv"), bl).unwrap();
}
