use std::{fs,path::PathBuf};
fn main(){
 let icons=PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("icons");
 fs::create_dir_all(&icons).unwrap();
 let mut icon=ico::IconDir::new(ico::ResourceType::Icon);
 for side in [16u32,32,48,256]{
  let mut px=vec![0u8;(side*side*4) as usize];
  for y in 0..side{for x in 0..side{
   let (u,v)=(64.0*x as f32/side as f32,64.0*y as f32/side as f32);
   let dx=u-u.clamp(13.0,51.0);let dy=v-v.clamp(13.0,51.0);
   if dx*dx+dy*dy>121.0{continue}
   let mut c=[27,66,113,255];
   if (12.0..=52.0).contains(&u)&&(14.0..=45.0).contains(&v){c=[232,245,255,255]}
   if (16.0..=48.0).contains(&u)&&(18.0..=41.0).contains(&v){c=[26,54,89,255]}
   if (20.0..=25.0).contains(&u)&&(31.0..=37.0).contains(&v)||(29.0..=34.0).contains(&u)&&(25.0..=37.0).contains(&v)||(38.0..=43.0).contains(&u)&&(21.0..=37.0).contains(&v){c=[67,220,169,255]}
   if (24.0..=40.0).contains(&u)&&(48.0..=53.0).contains(&v){c=[232,245,255,255]}
   let i=((y*side+x)*4) as usize;px[i..i+4].copy_from_slice(&c);
  }}
  icon.add_entry(ico::IconDirEntry::encode(&ico::IconImage::from_rgba_data(side,side,px)).unwrap());
 }
 icon.write(&mut fs::File::create(icons.join("icon.ico")).unwrap()).unwrap();
 tauri_build::build();
}
