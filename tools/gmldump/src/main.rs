use std::{fs,path::Path,io::Write};
fn s(p:&gm8exe::asset::PascalString)->String{ p.0.iter().map(|&b| b as char).collect() } // latin1
fn san(n:&str)->String{n.chars().map(|c| if c.is_alphanumeric()||c=='_'{c}else{'_'}).collect()}
fn acts(out:&mut String, a:&[gm8exe::asset::CodeAction]){
  for x in a { if x.action_kind==7 || x.id==603 { out.push_str(&s(&x.param_strings[0])); out.push_str("\n"); }
    else { out.push_str(&format!("// [D&D lib {} id {}] {} ({})\n", x.lib_id, x.id, s(&x.fn_name), (0..x.param_count).map(|i|s(&x.param_strings[i])).collect::<Vec<_>>().join(", "))); } }
}
fn main(){
  let a:Vec<String>=std::env::args().collect();
  let mut exe=fs::read(&a[1]).unwrap(); let o=Path::new(&a[2]);
  let g=gm8exe::reader::from_exe(&mut exe, None::<fn(&str)>, false, true).unwrap();
  for d in ["scripts","objects","rooms","timelines"]{fs::create_dir_all(o.join(d)).unwrap();}
  for sc in g.scripts.iter().flatten(){ fs::write(o.join("scripts").join(san(&s(&sc.name))+".gml"), s(&sc.source).as_bytes()).unwrap(); }
  let mut objtree=String::new();
  for (i,ob) in g.objects.iter().enumerate(){ if let Some(ob)=ob { let mut t=String::new();
    objtree.push_str(&format!("{}\t{}\tparent={}\tpersistent={}\n",i,s(&ob.name),ob.parent_index,ob.persistent));
    for (et,evs) in ob.events.iter().enumerate(){ for (sub,ac) in evs { t.push_str(&format!("\n///// EVENT type={} sub={}\n",et,sub)); acts(&mut t,ac);} }
    fs::write(o.join("objects").join(san(&s(&ob.name))+".gml"), t.as_bytes()).unwrap(); }}
  fs::write(o.join("objects.tsv"),objtree).unwrap();
  let mut rl=String::new();
  for r in g.rooms.iter().flatten(){ rl.push_str(&format!("{}\t{}x{}\tviews={}\tinst={}\n",s(&r.name),r.width,r.height,r.views_enabled,r.instances.len()));
    let mut t=s(&r.creation_code); for i in &r.instances { let c=s(&i.creation_code); if !c.trim().is_empty(){ t.push_str(&format!("\n//// inst {}\n{}",i.id,c)); } }
    if !t.trim().is_empty(){ fs::write(o.join("rooms").join(san(&s(&r.name))+".gml"), t.as_bytes()).unwrap(); } }
  fs::write(o.join("rooms.tsv"),rl).unwrap();
  for t in g.timelines.iter().flatten(){ let mut x=String::new(); for (m,ac) in &t.moments{ x.push_str(&format!("\n//// moment {}\n",m)); acts(&mut x,ac);} fs::write(o.join("timelines").join(san(&s(&t.name))+".gml"),x).unwrap(); }
  let mut f=fs::File::create(o.join("summary.txt")).unwrap();
  writeln!(f,"version {:?}\nsprites {} sounds {} backgrounds {} scripts {} objects {} rooms {} fonts {} timelines {} paths {}",g.version,g.sprites.iter().flatten().count(),g.sounds.iter().flatten().count(),g.backgrounds.iter().flatten().count(),g.scripts.iter().flatten().count(),g.objects.iter().flatten().count(),g.rooms.iter().flatten().count(),g.fonts.iter().flatten().count(),g.timelines.iter().flatten().count(),g.paths.iter().flatten().count()).unwrap();
  for e in &g.extensions { writeln!(f,"EXTENSION {}",s(&e.name)).unwrap(); for fl in &e.files { writeln!(f,"  file {} funcs {}",s(&fl.name),fl.functions.iter().map(|x|s(&x.name)).collect::<Vec<_>>().join(",")).unwrap(); } }
  for i in &g.included_files { writeln!(f,"INCLUDED {}",s(&i.file_name)).unwrap(); }
  for l in &g.library_init_strings { writeln!(f,"LIBINIT {}",s(l).lines().next().unwrap_or("")).unwrap(); }

  for sn in g.sounds.iter().flatten(){ writeln!(f,"SOUND {} ext={}",s(&sn.name),s(&sn.extension)).unwrap(); }
}
