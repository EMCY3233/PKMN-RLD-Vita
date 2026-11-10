//! Renderizador nulo: no dibuja nada. Sirve para medir la velocidad de la lógica del juego
//! (benchmark en PS Vita) y como base para el renderizador vitaGL.
//! Guarda solo las medidas de cada textura (las consulta la lógica), no los píxeles.
#![allow(unused_imports)]
use super::*;
use crate::types::Colour;
use atlas::{AtlasBuilder, AtlasRect, AtlasRef};
use std::any::Any;

pub struct RendererImpl {
    rects: Vec<Option<AtlasRect>>,
    size: (u32, u32),
    circle_precision: i32,
    blend: (BlendType, BlendType),
    model: [f32; 16],
    prim2d: PrimitiveBuilder,
    prim3d: PrimitiveBuilder,
}

impl RendererImpl {
    pub fn new(options: &RendererOptions) -> Self {
        let ident = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        Self {
            rects: Vec::new(),
            size: options.size,
            circle_precision: 24,
            blend: (BlendType::SrcAlpha, BlendType::InvSrcAlpha),
            model: ident,
            prim2d: PrimitiveBuilder::new(AtlasRect::default(), PrimitiveType::PointList, false),
            prim3d: PrimitiveBuilder::new(AtlasRect::default(), PrimitiveType::PointList, false),
        }
    }

    fn new_rect(&mut self, w: i32, h: i32) -> AtlasRef {
        self.rects.push(Some(AtlasRect { w, h, ..Default::default() }));
        AtlasRef(self.rects.len() as i32 - 1)
    }
}

impl RendererTrait for RendererImpl {
    #[allow(unused_variables)]
    fn as_any(&self) -> &dyn Any {
        self
    }

    #[allow(unused_variables)]
    fn max_texture_size(&self) -> u32 {
        4096
    }

    #[allow(unused_variables)]
    fn push_atlases(&mut self, atl: atlas::AtlasBuilder) -> Result<(), String> {
        let (_packers, sprites) = atl.into_inner();
        self.rects = sprites.into_iter().map(|(r, _)| Some(r)).collect();
        Ok(())
    }

    #[allow(unused_variables)]
    fn upload_sprite( &mut self, data: Box<[u8]>, width: i32, height: i32, origin_x: i32, origin_y: i32, ) -> Result<AtlasRef, String> {
        let _ = (data, origin_x, origin_y);
        Ok(self.new_rect(width, height))
    }

    #[allow(unused_variables)]
    fn duplicate_sprite(&mut self, atlas_ref: AtlasRef) -> Result<AtlasRef, String> {
        let r = self.get_rect(atlas_ref).copied().unwrap_or_default();
        Ok(self.new_rect(r.w, r.h))
    }

    #[allow(unused_variables)]
    fn delete_sprite(&mut self, atlas_ref: AtlasRef) {
        if let Some(r) = self.rects.get_mut(atlas_ref.0 as usize) { *r = None; }
    }

    #[allow(unused_variables)]
    fn resize_framebuffer(&mut self, width: u32, height: u32, store: bool) {
        let _ = store;
        self.size = (width, height);
    }

    #[allow(unused_variables)]
    fn set_vsync(&self, vsync: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_vsync(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn wait_vsync(&self) {
        
    }

    #[allow(unused_variables)]
    fn get_rect(&self, id: AtlasRef) -> Option<&AtlasRect> {
        if id.0 < 0 { None } else { self.rects.get(id.0 as usize).and_then(|r| r.as_ref()) }
    }

    #[allow(unused_variables)]
    fn draw_sprite_general( &mut self, texture: AtlasRef, part_x: f64, part_y: f64, part_w: f64, part_h: f64, x: f64, y: f64, xscale: f64, yscale: f64, angle: f64, col1: i32, col2: i32, col3: i32, col4: i32, alpha: f64, use_origin: bool, ) {
        
    }

    #[allow(unused_variables)]
    fn draw_sprite_pos( &mut self, texture: AtlasRef, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64, alpha: f64 ) {
        
    }

    #[allow(unused_variables)]
    fn set_view_matrix(&mut self, view: [f32; 16]) {
        
    }

    #[allow(unused_variables)]
    fn set_viewproj_matrix(&mut self, view: [f32; 16], proj: [f32; 16]) {
        
    }

    #[allow(unused_variables)]
    fn get_model_matrix(&self) -> [f32; 16] {
        self.model
    }

    #[allow(unused_variables)]
    fn set_model_matrix(&mut self, model: [f32; 16]) {
        self.model = model;
    }

    #[allow(unused_variables)]
    fn mult_model_matrix(&mut self, model: [f32; 16]) {
        
    }

    #[allow(unused_variables)]
    fn set_projection_ortho(&mut self, x: f64, y: f64, w: f64, h: f64, angle: f64) {
        
    }

    #[allow(unused_variables)]
    fn set_projection_perspective(&mut self, x: f64, y: f64, w: f64, h: f64, angle: f64) {
        
    }

    #[allow(unused_variables)]
    fn set_view( &mut self, src_x: i32, src_y: i32, src_w: i32, src_h: i32, src_angle: f64, port_x: i32, port_y: i32, port_w: i32, port_h: i32, ) {
        
    }

    #[allow(unused_variables)]
    fn flush_queue(&mut self) {
        
    }

    #[allow(unused_variables)]
    fn present(&mut self, window_width: u32, window_height: u32, scaling: Scaling) {
        
    }

    #[allow(unused_variables)]
    fn draw_stored(&mut self, target_x: i32, target_y: i32, width: u32, height: u32) {
        
    }

    #[allow(unused_variables)]
    fn stored_size(&self) -> (u32, u32) {
        (self.size.0, self.size.1)
    }

    #[allow(unused_variables)]
    fn finish(&mut self, window_width: u32, window_height: u32, clear_colour: Colour) {
        
    }

    #[allow(unused_variables)]
    fn dump_sprite_part(&self, texture: AtlasRef, part_x: i32, part_y: i32, part_w: i32, part_h: i32) -> Box<[u8]> {
        let _ = (texture, part_x, part_y);
        vec![0u8; (part_w.max(0) * part_h.max(0) * 4) as usize].into_boxed_slice()
    }

    #[allow(unused_variables)]
    fn get_alpha_blending(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_alpha_blending(&mut self, alphablend: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_colour_blending(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_colour_blending(&mut self, modulate: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_blend_mode(&self) -> (BlendType, BlendType) {
        self.blend
    }

    #[allow(unused_variables)]
    fn set_blend_mode(&mut self, src: BlendType, dst: BlendType) {
        self.blend = (src, dst);
    }

    #[allow(unused_variables)]
    fn get_pixel_interpolation(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_pixel_interpolation(&mut self, lerping: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_texture_repeat(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_texture_repeat(&mut self, repeat: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_pixels(&self, x: i32, y: i32, w: i32, h: i32) -> Box<[u8]> {
        let _ = (x, y);
        vec![0u8; (w.max(0) * h.max(0) * 4) as usize].into_boxed_slice()
    }

    #[allow(unused_variables)]
    fn stored_pixels(&self) -> Box<[u8]> {
        Default::default()
    }

    #[allow(unused_variables)]
    fn stored_zbuffer(&self) -> Box<[f32]> {
        Default::default()
    }

    #[allow(unused_variables)]
    fn set_stored(&mut self, rgba: Box<[u8]>, zbuf: Box<[f32]>, fb_w: u32, fb_h: u32) {
        
    }

    #[allow(unused_variables)]
    fn dump_dynamic_textures(&self) -> Vec<Option<SavedTexture>> {
        Default::default()
    }

    #[allow(unused_variables)]
    fn upload_dynamic_textures(&mut self, textures: &[Option<SavedTexture>]) {
        
    }

    #[allow(unused_variables)]
    fn create_sprite_colour(&mut self, width: i32, height: i32, col: Colour) -> Result<AtlasRef, String> {
        let _ = col;
        Ok(self.new_rect(width, height))
    }

    #[allow(unused_variables)]
    fn create_surface(&mut self, w: i32, h: i32, has_zbuffer: bool) -> Result<AtlasRef, String> {
        let _ = has_zbuffer;
        Ok(self.new_rect(w, h))
    }

    #[allow(unused_variables)]
    fn set_target(&mut self, atlas_ref: AtlasRef) {
        
    }

    #[allow(unused_variables)]
    fn reset_target(&mut self) {
        
    }

    #[allow(unused_variables)]
    fn copy_surface( &mut self, dest: AtlasRef, dest_x: i32, dest_y: i32, src: AtlasRef, src_x: i32, src_y: i32, width: i32, height: i32, ) {
        
    }

    #[allow(unused_variables)]
    fn set_zbuf_trashed(&mut self, trashed: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_zbuf_trashed(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn get_texture_id(&mut self, atl_ref: AtlasRef) -> i32 {
        atl_ref.0
    }

    #[allow(unused_variables)]
    fn get_texture_from_id(&self, id: i32) -> Option<AtlasRef> {
        if id >= 0 && (id as usize) < self.rects.len() { Some(AtlasRef(id)) } else { None }
    }

    #[allow(unused_variables)]
    fn get_texture_rects(&self) -> Vec<Option<AtlasRect>> {
        self.rects.clone()
    }

    #[allow(unused_variables)]
    fn set_texture_rects(&mut self, rects: &[Option<AtlasRect>]) {
        self.rects = rects.to_vec();
    }

    #[allow(unused_variables)]
    fn draw_rectangle(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: i32, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn draw_rectangle_outline(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: i32, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn draw_rectangle_gradient( &mut self, x1: f64, y1: f64, x2: f64, y2: f64, c1: i32, c2: i32, c3: i32, c4: i32, alpha: f64, outline: bool, ) {
        
    }

    #[allow(unused_variables)]
    fn draw_point(&mut self, x: f64, y: f64, colour: i32, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: Option<f64>, c1: i32, c2: i32, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn draw_triangle( &mut self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, c1: i32, c2: i32, c3: i32, alpha: f64, outline: bool, ) {
        
    }

    #[allow(unused_variables)]
    fn draw_ellipse(&mut self, x: f64, y: f64, rad_x: f64, rad_y: f64, c1: i32, c2: i32, alpha: f64, outline: bool) {
        
    }

    #[allow(unused_variables)]
    fn draw_roundrect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, c1: i32, c2: i32, alpha: f64, outline: bool) {
        
    }

    #[allow(unused_variables)]
    fn set_circle_precision(&mut self, prec: i32) {
        self.circle_precision = prec;
    }

    #[allow(unused_variables)]
    fn get_circle_precision(&self) -> i32 {
        self.circle_precision
    }

    #[allow(unused_variables)]
    fn reset_primitive_2d(&mut self, ptype: PrimitiveType, atlas_ref: Option<AtlasRef>) {
        let _ = (ptype, atlas_ref);
    }

    #[allow(unused_variables)]
    fn vertex_2d(&mut self, x: f64, y: f64, xtex: f64, ytex: f64, col: i32, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn draw_primitive_2d(&mut self) {
        
    }

    #[allow(unused_variables)]
    fn get_primitive_2d(&self) -> PrimitiveBuilder {
        self.prim2d.clone()
    }

    #[allow(unused_variables)]
    fn set_primitive_2d(&mut self, prim: PrimitiveBuilder) {
        self.prim2d = prim;
    }

    #[allow(unused_variables)]
    fn reset_primitive_3d(&mut self, ptype: PrimitiveType, atlas_ref: Option<AtlasRef>) {
        let _ = (ptype, atlas_ref);
    }

    #[allow(unused_variables)]
    fn vertex_3d( &mut self, x: f64, y: f64, z: f64, nx: f64, ny: f64, nz: f64, xtex: f64, ytex: f64, col: i32, alpha: f64, ) {
        
    }

    #[allow(unused_variables)]
    fn draw_primitive_3d(&mut self) {
        
    }

    #[allow(unused_variables)]
    fn get_primitive_3d(&self) -> PrimitiveBuilder {
        self.prim3d.clone()
    }

    #[allow(unused_variables)]
    fn set_primitive_3d(&mut self, prim: PrimitiveBuilder) {
        self.prim3d = prim;
    }

    #[allow(unused_variables)]
    fn extend_buffers(&self, buf: &mut VertexBuffer) {
        
    }

    #[allow(unused_variables)]
    fn draw_buffers(&mut self, atlas_ref: Option<AtlasRef>, buf: &VertexBuffer) {
        
    }

    #[allow(unused_variables)]
    fn clear_view(&mut self, colour: Colour, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn clear_view_no_zbuf(&mut self, colour: Colour, alpha: f64) {
        
    }

    #[allow(unused_variables)]
    fn clear_zbuf(&mut self) {
        
    }

    #[allow(unused_variables)]
    fn get_3d(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_3d(&mut self, use_3d: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_depth(&self) -> f32 {
        Default::default()
    }

    #[allow(unused_variables)]
    fn set_depth(&mut self, depth: f32) {
        
    }

    #[allow(unused_variables)]
    fn get_depth_test(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_depth_test(&mut self, depth_test: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_write_depth(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_write_depth(&mut self, write_depth: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_culling(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_culling(&mut self, culling: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_perspective(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_perspective(&mut self, perspective: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_fog(&self) -> Option<Fog> {
        Default::default()
    }

    #[allow(unused_variables)]
    fn set_fog(&mut self, fog: Option<Fog>) {
        
    }

    #[allow(unused_variables)]
    fn get_gouraud(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_gouraud(&mut self, gouraud: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_lighting_enabled(&self) -> bool {
        false
    }

    #[allow(unused_variables)]
    fn set_lighting_enabled(&mut self, enabled: bool) {
        
    }

    #[allow(unused_variables)]
    fn get_ambient_colour(&self) -> i32 {
        Default::default()
    }

    #[allow(unused_variables)]
    fn set_ambient_colour(&mut self, colour: i32) {
        
    }

    #[allow(unused_variables)]
    fn get_lights(&self) -> [(bool, Light); 8] {
        [(false, Light::Directional { direction: [0.0, 0.0, 1.0], colour: 0 }); 8]
    }

    #[allow(unused_variables)]
    fn set_lights(&mut self, lights: [(bool, Light); 8]) {
        
    }

    #[allow(unused_variables)]
    fn set_light_enabled(&mut self, id: usize, enabled: bool) {
        
    }

    #[allow(unused_variables)]
    fn set_light(&mut self, id: usize, light: Light) {
        
    }
}
