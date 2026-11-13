// roomdump <exe> <salida.tsv>: instancias de cada sala (sala, nombre, x, y, objeto, código de creación)
use std::{fs, io::Write};
fn s(p: &gm8exe::asset::PascalString) -> String { p.0.iter().map(|&b| b as char).collect() }
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut exe = fs::read(&a[1]).unwrap();
    let g = gm8exe::reader::from_exe(&mut exe, None::<fn(&str)>, false, true).unwrap();
    let mut out = fs::File::create(&a[2]).unwrap();
    let mut sc = fs::File::create(format!("{}.scripts", &a[2])).unwrap();
    for (i, x) in g.scripts.iter().enumerate() { if let Some(x) = x { writeln!(sc, "{}\t{}", i, s(&x.name)).unwrap(); } }
    let mut ob = fs::File::create(format!("{}.objects", &a[2])).unwrap();
    for (i, x) in g.objects.iter().enumerate() { if let Some(x) = x { writeln!(ob, "{}\t{}", i, s(&x.name)).unwrap(); } }
    for (ri, r) in g.rooms.iter().enumerate() {
        let Some(r) = r else { continue };
        for i in &r.instances {
            let cc = s(&i.creation_code).replace(['\n', '\r', '\t'], " ");
            writeln!(out, "{}\t{}\t{}\t{}\t{}\t{}", ri, s(&r.name), i.x, i.y, i.object, cc).unwrap();
        }
    }
}
