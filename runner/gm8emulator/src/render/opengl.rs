use crate::{
    render::{
        atlas::{AtlasBuilder, AtlasRect, AtlasRef},
        mat4mult, BlendType, Fog, Light, PrimitiveBuilder, PrimitiveShape, PrimitiveType, RendererOptions,
        RendererTrait, SavedTexture, Scaling, Vertex, VertexBuffer,
    },
    types::Colour,
};
use ramen::{connection::Connection, window::Window};
use crate::render::split_colour;
use rect_packer::DensePacker;
use std::{any::Any, f64::consts::PI, ffi::CStr, mem::{offset_of, size_of}, ptr};

/// Auto-generated OpenGL bindings from gl_generator
pub mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}
use gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};

#[cfg(target_os = "windows")]
mod wgl;
#[cfg(target_os = "windows")]
mod wgl_ffi;
#[cfg(target_os = "windows")]
use wgl as native_gl;
#[cfg(all(unix, not(target_os = "vita")))]
pub mod glx;
#[cfg(all(unix, not(target_os = "vita")))]
use glx as native_gl;
#[cfg(target_os = "vita")]
pub mod vgl;
#[cfg(target_os = "vita")]
use vgl as native_gl;

macro_rules! shader_file {
    ($path: expr) => {
        concat!(include_str!($path), "\0").as_bytes()
    };
}

// requires at least GL 3.3
pub const OPENGL_VERSION: (i32, i32) = (3, 3);

#[derive(Clone, Copy, PartialEq)]
#[repr(u32)]
enum GLBool {
    False = 0,
    True = 1,
}

impl From<GLBool> for bool {
    fn from(b: GLBool) -> Self {
        match b {
            GLBool::False => false,
            GLBool::True => true,
        }
    }
}

impl From<bool> for GLBool {
    fn from(b: bool) -> Self {
        if b { GLBool::True } else { GLBool::False }
    }
}

#[derive(Clone, Copy, PartialEq)]
#[repr(C)]
struct GLLight {
    pos: [f32; 4],
    colour: [f32; 4],
    enabled: GLBool,
    is_point: GLBool,
    range: f32,
    padding: u32, // must pad to size of vec4
}

#[derive(Clone, PartialEq)]
#[repr(C)]
struct RenderState {
    // vertex shader
    model_matrix: [f32; 16],
    viewproj_matrix: [f32; 16],
    lights: [GLLight; 8],
    ambient_colour: [f32; 4],
    lighting: GLBool,
    gouraud: GLBool,
    texture_blend: GLBool, // emulates glTexEnv(): GL_MODULATE if true, GL_REPLACE if false
    // frag shader
    texture_repeat: GLBool,
    interpolate_pixels: GLBool,
    depth_test: GLBool,
    fog_enabled: GLBool,
    fog_begin: f32,
    fog_end: f32,
    fog_colour: [f32; 4],
    // end of shader stuff
    end_of_uniform: (),
    view_matrix: [f32; 16],
    proj_matrix: [f32; 16],
    alpha_blending: bool,
    blend_mode: (BlendType, BlendType),
    write_depth: bool,
    culling: bool,
}

impl Default for RenderState {
    fn default() -> Self {
        // Create identity matrix to initialize MVP matrices with
        #[rustfmt::skip]
            let identity_matrix: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        Self {
            alpha_blending: true,
            blend_mode: (BlendType::SrcAlpha, BlendType::InvSrcAlpha),
            interpolate_pixels: GLBool::False,
            texture_repeat: GLBool::False,
            texture_blend: GLBool::True,
            model_matrix: identity_matrix.clone(),
            view_matrix: identity_matrix.clone(),
            proj_matrix: identity_matrix.clone(),
            depth_test: false.into(),
            write_depth: false,
            culling: false,
            fog_enabled: GLBool::False,
            fog_begin: 0.0,
            fog_end: 0.0,
            lighting: GLBool::False,
            gouraud: GLBool::True,
            ambient_colour: [0.0; 4],
            lights: [GLLight {
                pos: [0.0; 4],
                colour: [0.0; 4],
                enabled: GLBool::False,
                is_point: GLBool::False,
                range: 0.0,
                padding: 0,
            }; 8],
            viewproj_matrix: identity_matrix.clone(),
            fog_colour: [0.0; 4],
            end_of_uniform: (),
        }
    }
}

impl RenderState {
    fn update_matrix(&mut self, gl: &gl::Gl) {
        let mut viewport = [0; 4];
        unsafe {
            gl.GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
        }
        let offset_x = 1.0 / f64::from(viewport[2]);
        let offset_y = 1.0 / f64::from(viewport[3]);
        // build viewproj matrix
        #[rustfmt::skip]
            let viewproj = mat4mult(
            mat4mult(self.view_matrix, self.proj_matrix),
            // flip vertically because GL textures are flipped vertically vs DX
            // also GL's screen space is offset half a pixel vs DX so shift it
            [
                1.0,             0.0,             0.0, 0.0,
                0.0,             -1.0,            0.0, 0.0,
                0.0,             0.0,             1.0, 0.0,
                offset_x as f32, offset_y as f32, 0.0, 1.0,
            ],
        );
        self.viewproj_matrix = viewproj;
    }
}

pub struct RendererImpl {
    imp: native_gl::PlatformImpl,
    gl: gl::Gl,
    //program: GLuint,
    //vao: GLuint,
    atlas_packers: Vec<DensePacker>,
    texture_ids: Vec<Option<GLuint>>,
    zbuf_ids: Vec<Option<GLuint>>,
    fbo_ids: Vec<Option<GLuint>>,
    texture_rects: Vec<Option<AtlasRect>>,
    stock_texture_count: usize,
    stock_atlas_count: u32,
    current_atlas: u32,
    framebuffer: Framebuffer,
    stored_framebuffer: Option<Framebuffer>,
    zbuf_format: GLint,
    zbuf_trashed: bool,
    white_pixel: AtlasRect,
    vertex_queue: Vec<Vertex>,
    queue_type: PrimitiveShape,
    queue_render_state: RenderState,
    next_render_state: RenderState,
    render_state_updated: bool,
    circle_precision: i32,
    using_3d: bool,
    perspective: bool,
    depth: f32,
    primitive_2d: PrimitiveBuilder,
    primitive_3d: PrimitiveBuilder,

    simple: Option<SimpleUniforms>,
    lazy: LazyTextures,
    /// textura de la barra de teclas en pantalla (cifras 5×7 + bloque blanco), creada al usarla
    overlay_tex: GLuint,
    overlay_tex_size: (i32, i32),
    loc_gm81_normalize: GLint, // uniform bool gm81_normalize
    loc_tex: GLint,            // uniform sampler2D tex
    buf_state: GLuint,         // uniform RenderState state
}

#[derive(Clone, Copy)]
struct Framebuffer {
    pub texture: GLuint,
    pub zbuf: GLuint,
    pub fbo: GLuint,
}

static VERTEX_SHADER_SOURCE: &[u8] = shader_file!("glsl/vertex.glsl");
static FRAGMENT_SHADER_SOURCE: &[u8] = shader_file!("glsl/fragment.glsl");
// Shaders simplificados (2D, sin UBO ni iluminación) para vitaGL; en PC con RLD_SIMPLE_GL=1
static SIMPLE_VERTEX_SOURCE: &str = include_str!("glsl/simple_vertex.glsl");
static SIMPLE_FRAGMENT_SOURCE: &str = include_str!("glsl/simple_fragment.glsl");

/// ¿Usar el camino simplificado? Siempre en Vita; en PC solo para probarlo.
fn use_simple_gl() -> bool {
    cfg!(target_os = "vita") || std::env::var_os("RLD_SIMPLE_GL").is_some()
}

/// Texto final de un shader simplificado: en Vita tal cual (GLSL 1.00 → traductor de vitaGL);
/// en PC se adapta a GLSL 3.30 core.
fn simple_shader_source(src: &str, fragment: bool) -> std::ffi::CString {
    let prelude = if cfg!(target_os = "vita") {
        ""
    } else if fragment {
        "#version 330 core\n#define varying in\n#define texture2D texture\n#define gl_FragColor out_colour\nout vec4 out_colour;\n"
    } else {
        "#version 330 core\n#define attribute in\n#define varying out\n"
    };
    std::ffi::CString::new(format!("{}{}", prelude, src)).unwrap()
}

/// Texturas de carga diferida (PS Vita): los sprites del juego no caben a la vez en la memoria
/// gráfica (178 MB en RGBA frente a 112 MB de VRAM), así que se descomprimen al dibujarlos por
/// primera vez y se colocan en páginas de 1024×1024. Si se supera el presupuesto, se vacía la
/// página usada hace más tiempo (sus sprites se volverán a cargar si hacen falta).
#[derive(Default)]
struct LazyTextures {
    provider: Option<crate::render::LazyProvider>,
    /// AtlasRef → es diferida
    is_lazy: Vec<bool>,
    /// AtlasRef → página en la que está cargada (u32::MAX = no cargada)
    page_of: Vec<u32>,
    pages: Vec<LazyPage>,
    frame: u64,
    /// Píxeles BGRA de sprites creados en ejecución (sprite_add_sprite…) cuando van a las páginas
    /// diferidas en vez de a una textura propia con FBO (en vitaGL cada FBO cuesta memoria y
    /// adjuntarlos cerraba el juego)
    own: std::collections::HashMap<i32, Box<[u8]>>,
    budget_bytes: usize,
    used_bytes: usize,
    loads: u64,
    evictions: u64,
}

struct LazyPage {
    atlas_id: u32,
    packer: DensePacker,
    size: (i32, i32),
    ids: Vec<i32>,
    last_used: u64,
}

const LAZY_PAGE: i32 = 1024;

/// Ubicaciones de los uniforms del shader simplificado
#[derive(Clone, Copy, Default)]
struct SimpleUniforms {
    model: GLint,
    viewproj: GLint,
    colour_blending: GLint,
    tex_size: GLint,
    repeat: GLint,
    lerp: GLint,
    alpha_test: GLint,
    fog_enabled: GLint,
    fog_begin: GLint,
    fog_end: GLint,
    fog_colour: GLint,
}

unsafe fn shader_info_log(gl: &gl::Gl, name: &str, id: GLuint) -> String {
    let mut info_len: GLint = 0;
    gl.GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut info_len);
    let mut info = vec![0u8; info_len as usize];
    gl.GetShaderInfoLog(id, info_len as GLsizei, ptr::null_mut(), info.as_mut_ptr() as *mut GLchar);
    info.set_len((info_len - 1) as usize); // ignore null for str::from_utf8
    format!(
        "Failed to compile {} shader, compiler output:\n{}",
        name,
        std::str::from_utf8(&info).unwrap_or("<INVALID UTF-8>")
    )
}

fn make_view_matrix(x: f64, y: f64, z: f64, w: f64, h: f64, angle: f64) -> [f32; 16] {
    // Note: sin is negated because it's the same as negating the angle, which is how GM8 does view angles
    let angle = angle.to_radians();
    let sin_angle = -angle.sin() as f32;
    let cos_angle = angle.cos() as f32;

    #[rustfmt::skip]
    let view_matrix: [f32; 16] = {
        // source rectangle's center coordinates aka -(x + w/2) and -(y + h/2)
        let scx = -((x as f32) + (w as f32 / 2.0));
        let scy = -((y as f32) + (h as f32 / 2.0));
        let scz = -z as f32;
        mat4mult(
            // Place camera at (scx, scy, scz)
            [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                scx, scy, scz, 1.0,
            ],
            // Rotate to view_angle
            [
                cos_angle,  sin_angle, 0.0, 0.0,
                -sin_angle, cos_angle, 0.0, 0.0,
                0.0,        0.0,       1.0, 0.0,
                0.0,        0.0,       0.0, 1.0,
            ]
        )
    };

    view_matrix
}


#[derive(Debug)]
struct LightUniform {
    enabled: GLint,
    is_point: GLint,
    pos: GLint,
    colour: GLint,
    range: GLint,
}

impl From<BlendType> for GLenum {
    fn from(bt: BlendType) -> Self {
        match bt {
            BlendType::Zero => gl::ZERO,
            BlendType::One => gl::ONE,
            BlendType::SrcColour => gl::SRC_COLOR,
            BlendType::InvSrcColour => gl::ONE_MINUS_SRC_COLOR,
            BlendType::SrcAlpha => gl::SRC_ALPHA,
            BlendType::InvSrcAlpha => gl::ONE_MINUS_SRC_ALPHA,
            BlendType::DestAlpha => gl::DST_ALPHA,
            BlendType::InvDestAlpha => gl::ONE_MINUS_DST_ALPHA,
            BlendType::DestColour => gl::DST_COLOR,
            BlendType::InvDestColour => gl::ONE_MINUS_DST_COLOR,
            BlendType::SrcAlphaSaturate => gl::SRC_ALPHA_SATURATE,
        }
    }
}

impl From<GLenum> for BlendType {
    fn from(bt: GLenum) -> Self {
        match bt {
            gl::ZERO => BlendType::Zero,
            gl::ONE => BlendType::One,
            gl::SRC_COLOR => BlendType::SrcColour,
            gl::ONE_MINUS_SRC_COLOR => BlendType::InvSrcColour,
            gl::SRC_ALPHA => BlendType::SrcAlpha,
            gl::ONE_MINUS_SRC_ALPHA => BlendType::InvSrcAlpha,
            gl::DST_ALPHA => BlendType::DestAlpha,
            gl::ONE_MINUS_DST_ALPHA => BlendType::InvDestAlpha,
            gl::DST_COLOR => BlendType::DestColour,
            gl::ONE_MINUS_DST_COLOR => BlendType::InvDestColour,
            gl::SRC_ALPHA_SATURATE => BlendType::SrcAlphaSaturate,
            _ => unreachable!(),
        }
    }
}

impl From<PrimitiveShape> for GLenum {
    fn from(shape: PrimitiveShape) -> Self {
        match shape {
            PrimitiveShape::Point => gl::POINTS,
            PrimitiveShape::Line => gl::LINES,
            PrimitiveShape::Triangle => gl::TRIANGLES,
        }
    }
}

/// A builder to be used for building basic shapes.
struct ShapeBuilder {
    primitive: PrimitiveBuilder,
    outline: bool,
    depth: f32,
    alpha: f64,
}

impl ShapeBuilder {
    fn new(outline: bool, atlas_ref: AtlasRect, alpha: f64, depth: f32) -> Self {
        Self {
            primitive: PrimitiveBuilder::new(
                atlas_ref,
                if outline { PrimitiveType::LineStrip } else { PrimitiveType::TriFan },
                false,
            ),
            outline,
            depth,
            alpha,
        }
    }

    /// Shortcut for basic shapes.
    fn push_point(&mut self, x: f64, y: f64, colour: i32) -> &mut Self {
        self.primitive.push_vertex([x as f32, y as f32, self.depth], [0.0, 0.0], split_colour(colour, self.alpha), [
            0.0, 0.0, 0.0,
        ]);
        self
    }

    /// Should only be called once. This is only used for basic shapes, so it's fine for it to be *possible* to
    /// call it multiple times, as that makes things easier elsewhere.
    fn build(&mut self) -> &PrimitiveBuilder {
        if self.outline {
            let vertices = self.primitive.get_vertices();
            if vertices.len() > 2 {
                let vertex = vertices[0];
                self.primitive.push_vertex_raw(vertex);
            }
        }
        &self.primitive
    }
}

// TODO: Implement Drop trait for RendererImpl to delete OpenGL objects we create? This doesn't make
// much sense in Release builds - because then we're doing the OS's work for it and just increasing
// the process termination time - but can be quite useful for Debug ones.

impl RendererImpl {
    pub fn new(options: &RendererOptions, connection: &Connection, window: &Window, clear_colour: Colour) -> Result<Self, String> {
        unsafe {
            let imp = native_gl::PlatformImpl::new(connection, window)?;

            // gl function pointers
            let gl = gl::Gl::load_with(native_gl::PlatformImpl::get_function_loader()?);
            native_gl::PlatformImpl::clean_function_loader();

            // debug print
            let ver_str = CStr::from_ptr(gl.GetString(gl::VERSION).cast()).to_str().unwrap();
            let vendor_str = CStr::from_ptr(gl.GetString(gl::VENDOR).cast()).to_str().unwrap();
            eprintln!("creating graphics context\n  > GL_VERSION: \"{}\"\n  > GL_VENDOR: \"{}\"", ver_str, vendor_str);

            #[cfg(not(target_os = "vita"))]
            {
            let mut v_maj: GLint = 0;
            let mut v_min: GLint = 0;
            gl.GetIntegerv(gl::MAJOR_VERSION, &mut v_maj);
            gl.GetIntegerv(gl::MINOR_VERSION, &mut v_min);
            assert!(
                (v_maj == OPENGL_VERSION.0 && v_min >= OPENGL_VERSION.1) || v_maj > OPENGL_VERSION.0,
                "OpenGL version {}.{} or later is required, but found version {}.{}",
                OPENGL_VERSION.0,
                OPENGL_VERSION.1,
                v_maj,
                v_min
            );
            }

            imp.set_swap_interval(if options.vsync { 1 } else { 0 });
            let zbuf_format = if options.zbuf_24 { gl::DEPTH_COMPONENT24 } else { gl::DEPTH_COMPONENT16 } as GLint;

            // Compile vertex shader
            let simple = use_simple_gl();
            let simple_vs = simple_shader_source(SIMPLE_VERTEX_SOURCE, false);
            let simple_fs = simple_shader_source(SIMPLE_FRAGMENT_SOURCE, true);
            let vertex_shader = gl.CreateShader(gl::VERTEX_SHADER);
            let vs_ptr: *const GLchar = if simple { simple_vs.as_ptr() } else { VERTEX_SHADER_SOURCE.as_ptr().cast() };
            gl.ShaderSource(vertex_shader, 1, &vs_ptr, ptr::null());
            gl.CompileShader(vertex_shader);

            // Check for vertex shader compile errors
            let mut success = gl::FALSE as GLint;
            gl.GetShaderiv(vertex_shader, gl::COMPILE_STATUS, &mut success);
            if success != gl::TRUE as GLint {
                return Err(shader_info_log(&gl, "vertex", vertex_shader))
            }

            // Compile fragment shader
            let fragment_shader = gl.CreateShader(gl::FRAGMENT_SHADER);
            let fs_ptr: *const GLchar = if simple { simple_fs.as_ptr() } else { FRAGMENT_SHADER_SOURCE.as_ptr().cast() };
            gl.ShaderSource(fragment_shader, 1, &fs_ptr, ptr::null());
            gl.CompileShader(fragment_shader);

            // Check for fragment shader compile errors
            gl.GetShaderiv(fragment_shader, gl::COMPILE_STATUS, &mut success);
            if success != gl::TRUE as GLint {
                return Err(shader_info_log(&gl, "fragment", fragment_shader))
            }

            // Link shaders
            let program = gl.CreateProgram();
            gl.AttachShader(program, vertex_shader);
            gl.AttachShader(program, fragment_shader);
            if simple {
                // mismas posiciones que layout(location = N) en los shaders originales
                for (i, n) in [(0, "pos\0"), (1, "blend\0"), (2, "tex_coord\0"), (4, "atlas_xywh\0")] {
                    gl.BindAttribLocation(program, i, n.as_ptr().cast());
                }
            }
            gl.LinkProgram(program);

            // Check for linking errors
            // TODO: generalize this like with shader info logs!! please!!!
            gl.GetProgramiv(program, gl::LINK_STATUS, &mut success);
            if success != gl::TRUE as GLint {
                let mut info_len: GLint = 0;
                gl.GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut info_len);
                let mut info = vec![0u8; info_len as usize];
                gl.GetProgramInfoLog(program, info_len as GLsizei, ptr::null_mut(), info.as_mut_ptr() as *mut GLchar);
                info.set_len((info_len - 1) as usize); // ignore null for str::from_utf8
                return Err(format!(
                    "Failed to link shaders, compiler output:\n{}",
                    std::str::from_utf8(&info).unwrap_or("<INVALID UTF-8>")
                ))
            }
            gl.DeleteShader(vertex_shader);
            gl.DeleteShader(fragment_shader);

            // set up vertex array
            let mut vao = 0;
            gl.GenVertexArrays(1, &mut vao);
            gl.BindVertexArray(vao);

            // Enable and disable GL features
            gl.Enable(gl::SCISSOR_TEST);
            // gl::Enable(gl::TEXTURE_2D);
            gl.Disable(gl::CULL_FACE);
            gl.Enable(gl::BLEND);
            gl.Disable(gl::DEPTH_TEST);

            gl.BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl.DepthFunc(gl::LEQUAL);

            // Use DX provoking vertex convention
            gl.ProvokingVertex(gl::FIRST_VERTEX_CONVENTION);

            // Create and bind the VBO once
            let mut commands_vbo: GLuint = 0;
            gl.GenBuffers(1, &mut commands_vbo);
            gl.BindBuffer(gl::ARRAY_BUFFER, commands_vbo);

            // layout (location = 0) in vec3 pos;
            // layout (location = 1) in vec4 blend;
            // layout (location = 2) in vec2 tex_coord;
            // layout (location = 3) in vec3 normal;
            // layout (location = 4) in vec4 atlas_xywh;
            gl.EnableVertexAttribArray(0);
            gl.VertexAttribPointer(
                0,
                3,
                gl::FLOAT,
                gl::FALSE,
                size_of::<Vertex>() as i32,
                offset_of!(Vertex, pos) as *const _,
            );
            gl.EnableVertexAttribArray(1);
            gl.VertexAttribPointer(
                1,
                4,
                gl::FLOAT,
                gl::FALSE,
                size_of::<Vertex>() as i32,
                offset_of!(Vertex, blend) as *const _,
            );
            gl.EnableVertexAttribArray(2);
            gl.VertexAttribPointer(
                2,
                2,
                gl::FLOAT,
                gl::FALSE,
                size_of::<Vertex>() as i32,
                offset_of!(Vertex, tex_coord) as *const _,
            );
            if !simple { gl.EnableVertexAttribArray(3); }
            gl.VertexAttribPointer(
                3,
                3,
                gl::FLOAT,
                gl::FALSE,
                size_of::<Vertex>() as i32,
                offset_of!(Vertex, normal) as *const _,
            );
            gl.EnableVertexAttribArray(4);
            gl.VertexAttribPointer(
                4,
                4,
                gl::FLOAT,
                gl::FALSE,
                size_of::<Vertex>() as i32,
                offset_of!(Vertex, atlas_xywh) as *const _,
            );

            // Use program
            gl.UseProgram(program);

            // Configure gl::ReadPixels() to read from the back buffer
            gl.ReadBuffer(gl::BACK);

            // Configure gl::ReadPixels() to align to 1 byte
            gl.PixelStorei(gl::PACK_ALIGNMENT, 1);

            // Create framebuffer
            let (mut framebuffer_texture, mut framebuffer_zbuf, mut framebuffer_fbo) = (0, 0, 0);
            gl.GenTextures(1, &mut framebuffer_texture);
            gl.BindTexture(gl::TEXTURE_2D, framebuffer_texture);
            gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            gl.TexImage2D(
                gl::TEXTURE_2D,      // target
                0,                   // level
                gl::RGBA as _,       // internalformat
                options.size.0 as _, // width
                options.size.1 as _, // height
                0,                   // border ("must be 0")
                gl::RGBA,            // format
                gl::UNSIGNED_BYTE,   // type
                ptr::null(),         // data
            );
            gl.GenTextures(1, &mut framebuffer_zbuf);
            gl.BindTexture(gl::TEXTURE_2D, framebuffer_zbuf);
            gl.TexImage2D(
                gl::TEXTURE_2D,      // target
                0,                   // level
                zbuf_format,         // internalformat
                options.size.0 as _, // width
                options.size.1 as _, // height
                0,                   // border ("must be 0")
                gl::DEPTH_COMPONENT, // format
                gl::FLOAT,           // type
                ptr::null(),         // data
            );
            gl.GenFramebuffers(1, &mut framebuffer_fbo);
            gl.BindFramebuffer(gl::FRAMEBUFFER, framebuffer_fbo);
            gl.FramebufferTexture2D(
                gl::READ_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                framebuffer_texture,
                0,
            );
            gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, framebuffer_zbuf, 0);

            // setup uniform buffer for render state
            let mut buf_state: GLuint = 0;
            let simple_uniforms = if simple {
                let u = |n: &str| {
                    let c = std::ffi::CString::new(n).unwrap();
                    gl.GetUniformLocation(program, c.as_ptr())
                };
                Some(SimpleUniforms {
                    model: u("model"),
                    viewproj: u("viewproj"),
                    colour_blending: u("colour_blending"),
                    tex_size: u("tex_size"),
                    repeat: u("repeat"),
                    lerp: u("interp"),
                    alpha_test: u("alpha_test"),
                    fog_enabled: u("fog_enabled"),
                    fog_begin: u("fog_begin"),
                    fog_end: u("fog_end"),
                    fog_colour: u("fog_colour"),
                })
            } else {
                let loc_state = 1;
                let ind = gl.GetUniformBlockIndex(program, b"RenderState\0".as_ptr().cast());
                assert_ne!(ind, gl::INVALID_INDEX);
                gl.UniformBlockBinding(program, ind, loc_state);
                gl.GenBuffers(1, &mut buf_state);
                gl.BindBufferBase(gl::UNIFORM_BUFFER, loc_state, buf_state);
                None
            };
            assert_eq!(gl.GetError(), 0);

            // Create Renderer
            let mut renderer = Self {
                imp,
                //program,
                //vao,
                //commands_vbo,
                atlas_packers: vec![],
                texture_ids: vec![],
                zbuf_ids: vec![],
                fbo_ids: vec![],
                texture_rects: vec![],
                stock_texture_count: 0,
                stock_atlas_count: 0,
                current_atlas: 0,
                framebuffer: Framebuffer { texture: framebuffer_texture, zbuf: framebuffer_zbuf, fbo: framebuffer_fbo },
                stored_framebuffer: None,
                zbuf_format,
                zbuf_trashed: false,
                white_pixel: Default::default(),
                vertex_queue: Vec::with_capacity(1536),
                queue_type: PrimitiveShape::Triangle,
                queue_render_state: RenderState {
                    interpolate_pixels: options.interpolate_pixels.into(),
                    ..Default::default()
                },
                next_render_state: RenderState {
                    interpolate_pixels: options.interpolate_pixels.into(),
                    ..Default::default()
                },
                render_state_updated: false,
                circle_precision: 24,
                using_3d: false,
                perspective: false,
                depth: 0.0,
                primitive_2d: PrimitiveBuilder::new(Default::default(), PrimitiveType::PointList, false),
                primitive_3d: PrimitiveBuilder::new(Default::default(), PrimitiveType::PointList, false),

                simple: simple_uniforms,
                overlay_tex: 0,
                overlay_tex_size: (1, 1),
                lazy: LazyTextures {
                    budget_bytes: std::env::var("RLD_TEX_BUDGET_MB")
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(if cfg!(target_os = "vita") { 72 } else { 4096 })
                        * 1024
                        * 1024,
                    ..Default::default()
                },
                loc_gm81_normalize: gl.GetUniformLocation(program, b"gm81_normalize\0".as_ptr().cast()),
                loc_tex: gl.GetUniformLocation(program, b"tex\0".as_ptr().cast()),
                buf_state,

                gl,
            };
            assert_eq!(renderer.gl.GetError(), 0);

            // default uniform values
            renderer.gl.Uniform1i(renderer.loc_gm81_normalize, options.normalize_normals as _);
            renderer.gl.Uniform1i(renderer.loc_tex, 0 as _);
            renderer.upload_state();
            assert_eq!(renderer.gl.GetError(), 0);

            // Start first frame
            renderer.setup_frame(clear_colour);

            // verify it actually worked
            match renderer.gl.GetError() {
                0 => Ok(renderer),
                err => Err(format!("Failed to fully initialize OpenGL! (OpenGL code {})", err)),
            }
        }
    }

    /// ¿Los sprites creados en ejecución van a las páginas diferidas? (Vita, o PC con RLD_LAZY_TEX)
    fn dynamic_in_pages(&self) -> bool {
        self.lazy.provider.is_some()
    }

    /// Registra un sprite creado en ejecución (píxeles BGRA) como textura diferida propia
    fn register_own(&mut self, bgra: Box<[u8]>, w: i32, h: i32, origin_x: f32, origin_y: f32) -> AtlasRef {
        let id = self.texture_rects.len();
        self.texture_rects.push(Some(AtlasRect {
            atlas_id: crate::render::atlas::LAZY_UNLOADED,
            x: 0,
            y: 0,
            w,
            h,
            origin_x,
            origin_y,
        }));
        if self.lazy.is_lazy.len() < id {
            self.lazy.is_lazy.resize(id, false);
            self.lazy.page_of.resize(id, u32::MAX);
        }
        self.lazy.is_lazy.push(true);
        self.lazy.page_of.push(u32::MAX);
        self.lazy.own.insert(id as i32, bgra);
        AtlasRef(id as i32)
    }

    /// Píxeles BGRA de una textura diferida (propia o del juego)
    fn lazy_pixels(&mut self, r: AtlasRef) -> Option<Box<[u8]>> {
        if let Some(px) = self.lazy.own.get(&r.0) {
            return Some(px.clone())
        }
        let mut provider = self.lazy.provider.take()?;
        let res = provider(r).into_iter().find(|(x, _)| x.0 == r.0).map(|(_, p)| p);
        self.lazy.provider = Some(provider);
        res
    }

    /// Carga en la GPU una textura diferida si aún no lo está (y la marca como usada)
    fn ensure(&mut self, atlas_ref: AtlasRef) {
        let Ok(i) = usize::try_from(atlas_ref.0) else { return };
        if !self.lazy.is_lazy.get(i).copied().unwrap_or(false) {
            return
        }
        let page = self.lazy.page_of[i];
        if page != u32::MAX {
            self.lazy.pages[page as usize].last_used = self.lazy.frame;
            return
        }
        if let Some(px) = self.lazy.own.remove(&atlas_ref.0) {
            if let Some(rect) = self.texture_rects[i] {
                self.upload_lazy(i, rect, &px);
            }
            self.lazy.own.insert(atlas_ref.0, px);
            return
        }
        let Some(mut provider) = self.lazy.provider.take() else { return };
        let loaded = provider(atlas_ref);
        self.lazy.provider = Some(provider);
        self.lazy.loads += 1;
        for (r, pixels) in loaded {
            let Ok(j) = usize::try_from(r.0) else { continue };
            if !self.lazy.is_lazy.get(j).copied().unwrap_or(false) || self.lazy.page_of[j] != u32::MAX {
                continue
            }
            let Some(rect) = self.texture_rects[j] else { continue };
            if pixels.len() != (rect.w * rect.h * 4) as usize {
                continue
            }
            self.upload_lazy(j, rect, &pixels);
        }
    }

    fn upload_lazy(&mut self, id: usize, rect: AtlasRect, pixels: &[u8]) {
        let (w, h) = (rect.w, rect.h);
        let frame = self.lazy.frame;
        // 1) hueco en una página existente
        let mut found = None;
        for (pi, p) in self.lazy.pages.iter_mut().enumerate() {
            if let Some(r) = p.packer.pack(w, h, false) {
                found = Some((pi, r));
                break
            }
        }
        // 2) vaciar la página usada hace más tiempo (si no cabe otra en el presupuesto)
        if found.is_none() {
            let need = {
                let pw = LAZY_PAGE.max((w as u32).next_power_of_two() as i32).min(self.max_texture_size() as i32);
                let ph = LAZY_PAGE.max((h as u32).next_power_of_two() as i32).min(self.max_texture_size() as i32);
                (pw, ph)
            };
            if self.lazy.used_bytes + (need.0 * need.1 * 4) as usize > self.lazy.budget_bytes {
                let victim = self
                    .lazy
                    .pages
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.last_used < frame && p.size.0 >= w && p.size.1 >= h)
                    .min_by_key(|(_, p)| p.last_used)
                    .map(|(i, _)| i);
                if let Some(v) = victim {
                    self.flush_queue();
                    let page = &mut self.lazy.pages[v];
                    for &old in &page.ids {
                        self.lazy.page_of[old as usize] = u32::MAX;
                        if let Some(r) = self.texture_rects[old as usize].as_mut() {
                            r.atlas_id = crate::render::atlas::LAZY_UNLOADED;
                        }
                    }
                    page.ids.clear();
                    page.packer = DensePacker::new(page.size.0, page.size.1);
                    self.lazy.evictions += 1;
                    if self.lazy.evictions <= 3 || self.lazy.evictions % 100 == 0 {
                        crate::startlog(&format!(
                            "Texturas: página vaciada (vaciados: {}, cargas: {}, {} MB en uso)",
                            self.lazy.evictions,
                            self.lazy.loads,
                            self.lazy.used_bytes / 1048576
                        ));
                    }
                    if let Some(r) = page.packer.pack(w, h, false) {
                        found = Some((v, r));
                    }
                }
            }
            // 3) página nueva (también si todo lo cargado se usa en este mismo fotograma)
            if found.is_none() {
                let (pw, ph) = need;
                let mut tex = 0;
                unsafe {
                    self.flush_queue();
                    self.gl.GenTextures(1, &mut tex);
                    self.gl.BindTexture(gl::TEXTURE_2D, tex);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as _);
                    self.gl.TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, pw, ph, 0, gl::BGRA, gl::UNSIGNED_BYTE, ptr::null());
                }
                let atlas_id = self.texture_ids.len() as u32;
                self.texture_ids.push(Some(tex));
                self.fbo_ids.resize(self.texture_ids.len(), None);
                self.zbuf_ids.resize(self.texture_ids.len(), None);
                self.lazy.used_bytes += (pw * ph * 4) as usize;
                let mut packer = DensePacker::new(pw, ph);
                let r = packer.pack(w, h, false);
                self.lazy.pages.push(LazyPage { atlas_id, packer, size: (pw, ph), ids: Vec::new(), last_used: frame });
                match r {
                    Some(r) => found = Some((self.lazy.pages.len() - 1, r)),
                    None => return, // más grande que la textura máxima: no se puede dibujar
                }
            }
        }
        let Some((pi, r)) = found else { return };
        let page = &mut self.lazy.pages[pi];
        page.ids.push(id as i32);
        page.last_used = frame;
        let atlas_id = page.atlas_id;
        self.lazy.page_of[id] = pi as u32;
        unsafe {
            self.gl.BindTexture(gl::TEXTURE_2D, self.texture_ids[atlas_id as usize].unwrap());
            self.gl.TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                r.x,
                r.y,
                w,
                h,
                gl::BGRA,
                gl::UNSIGNED_BYTE,
                pixels.as_ptr().cast(),
            );
        }
        if let Some(t) = self.texture_rects[id].as_mut() {
            t.atlas_id = atlas_id;
            t.x = r.x;
            t.y = r.y;
        }
    }

    /// Textura de la fuente del runner (vita/font.png: celdas de 10×20, Latin-1 32..255; la 127 es
    /// un bloque blanco para fondos)
    unsafe fn overlay_font(&mut self) -> (i32, i32) {
        const PNG: &[u8] = include_bytes!("../../../../vita/font.png");
        if self.overlay_tex == 0 {
            let img = image::load_from_memory(PNG).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
            let (w, h) = (img.width() as i32, img.height() as i32);
            self.gl.GenTextures(1, &mut self.overlay_tex);
            self.gl.BindTexture(gl::TEXTURE_2D, self.overlay_tex);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            self.gl.TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as _, w, h, 0, gl::RGBA, gl::UNSIGNED_BYTE, img.as_ptr().cast());
            self.overlay_tex_size = (w, h);
        }
        self.overlay_tex_size
    }

    /// Dibuja lo que el runner pone encima del juego: página a pantalla completa o barra de teclas
    unsafe fn draw_overlay(&mut self, u: SimpleUniforms, win_w: i32, win_h: i32) {
        let page = crate::render::OVERLAY_PAGE.lock().ok().and_then(|p| p.clone());
        let bar = crate::render::OVERLAY_VISIBLE.load(std::sync::atomic::Ordering::Relaxed);
        if page.is_none() && !bar {
            return
        }
        let (tw, th) = self.overlay_font();
        let (ww, wh) = (win_w as f32, win_h as f32);
        let to_clip = move |x: i32, y: i32| (x as f32 / ww * 2.0 - 1.0, 1.0 - y as f32 / wh * 2.0);
        let quad = move |verts: &mut Vec<Vertex>, x: i32, y: i32, w: i32, h: i32, atlas: [f32; 4], col: [f32; 4]| {
            let (x1, y1) = to_clip(x, y);
            let (x2, y2) = to_clip(x + w, y + h);
            let v = |px: f32, py: f32, tx: f32, ty: f32| Vertex {
                pos: [px, py, 0.0],
                tex_coord: [tx, ty],
                blend: col,
                atlas_xywh: atlas,
                normal: [0.0; 3],
            };
            verts.extend_from_slice(&[
                v(x1, y1, 0.0, 0.0),
                v(x2, y1, 1.0, 0.0),
                v(x1, y2, 0.0, 1.0),
                v(x2, y1, 1.0, 0.0),
                v(x2, y2, 1.0, 1.0),
                v(x1, y2, 0.0, 1.0),
            ]);
        };
        let cell = |c: u8| -> [f32; 4] {
            let i = (c.max(32) - 32) as i32;
            [((i % 16) * 10) as f32, ((i / 16) * 20) as f32, 10.0, 20.0]
        };
        let block = cell(127);
        let rect = |verts: &mut Vec<Vertex>, x: i32, y: i32, w: i32, h: i32, col: [f32; 4]| {
            quad(verts, x, y, w, h, [block[0] + 2.0, block[1] + 2.0, 4.0, 4.0], col)
        };
        let text = |verts: &mut Vec<Vertex>, x: i32, y: i32, s: &str, col: [f32; 4]| {
            let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(s);
            for (k, &b) in bytes.iter().enumerate() {
                if b != b' ' {
                    quad(verts, x + k as i32 * 10, y, 10, 20, cell(b), col);
                }
            }
        };
        let mut verts: Vec<Vertex> = Vec::new();
        let mut image: Option<(AtlasRef, i32, i32, i32, i32)> = None;
        if let Some(p) = &page {
            rect(&mut verts, 0, 0, win_w, win_h, [0.02, 0.03, 0.08, 0.93]);
            rect(&mut verts, 0, 0, win_w, 30, [0.15, 0.25, 0.55, 1.0]);
            text(&mut verts, 12, 5, &p.title, [1.0, 0.9, 0.4, 1.0]);
            if let Some(img) = p.image {
                // imagen centrada, escalada para caber entre título y pie
                self.ensure(img);
                if let Some(r) = self.get_rect(img).copied() {
                    let (aw, ah) = (win_w - 20, win_h - 70);
                    let s = ((aw as f32 / r.w as f32).min(ah as f32 / r.h as f32)).max(0.01);
                    let s = if s >= 1.0 { s.floor() } else { s };
                    let (w, h) = ((r.w as f32 * s) as i32, (r.h as f32 * s) as i32);
                    image = Some((img, (win_w - w) / 2, 34 + (ah - h) / 2, w, h));
                }
            }
            for (k, line) in p.lines.iter().skip(p.scroll).take(crate::render::PAGE_LINES).enumerate() {
                let y = 38 + k as i32 * 22;
                let idx = p.scroll + k;
                if p.selected == Some(idx) {
                    rect(&mut verts, 6, y - 1, win_w - 12, 22, [0.95, 0.75, 0.2, 0.35]);
                }
                text(&mut verts, 14, y, line, [1.0, 1.0, 1.0, 1.0]);
            }
            if p.lines.len() > crate::render::PAGE_LINES {
                let more = format!("{}-{} / {}", p.scroll + 1, (p.scroll + crate::render::PAGE_LINES).min(p.lines.len()), p.lines.len());
                text(&mut verts, win_w - 14 - more.len() as i32 * 10, 5, &more, [0.8, 0.8, 0.8, 1.0]);
            }
            rect(&mut verts, 0, win_h - 28, win_w, 28, [0.15, 0.25, 0.55, 1.0]);
            text(&mut verts, 12, win_h - 24, &p.footer, [1.0, 1.0, 1.0, 1.0]);
        } else {
            let pressed = crate::render::OVERLAY_PRESSED.load(std::sync::atomic::Ordering::Relaxed);
            for i in 0..crate::render::OVERLAY_KEYS.len() {
                let (x, y, w, h) = crate::render::overlay_button(i);
                let bg = if i as i32 == pressed { [0.95, 0.75, 0.2, 0.85] } else { [0.08, 0.1, 0.18, 0.5] };
                rect(&mut verts, x, y, w, h, bg);
                let label = crate::render::OVERLAY_LABELS[i];
                // texto a doble tamaño, centrado
                let (bytes, _, _) = encoding_rs::WINDOWS_1252.encode(label);
                let lw = bytes.len() as i32 * 20;
                for (k, &b) in bytes.iter().enumerate() {
                    quad(&mut verts, x + (w - lw) / 2 + k as i32 * 20, y + (h - 40) / 2, 20, 40, cell(b), [1.0, 1.0, 1.0, 0.95]);
                }
            }
        }
        self.gl.Uniform1f(u.colour_blending, 1.0);
        self.gl.Enable(gl::BLEND);
        self.gl.BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        // 1) fondos y textos (textura de la fuente)
        let mut draw = |gl: &gl::Gl, tex: GLuint, size: (i32, i32), verts: &[Vertex]| {
            if verts.is_empty() {
                return
            }
            gl.BindTexture(gl::TEXTURE_2D, tex);
            gl.Uniform2f(u.tex_size, size.0 as f32, size.1 as f32);
            gl.BufferData(gl::ARRAY_BUFFER, (size_of::<Vertex>() * verts.len()) as _, verts.as_ptr().cast(), gl::STREAM_DRAW);
            gl.DrawArrays(gl::TRIANGLES, 0, verts.len() as i32);
        };
        draw(&self.gl, self.overlay_tex, (tw, th), &verts);
        // 2) imagen de la página (textura del sprite)
        if let Some((img, x, y, w, h)) = image {
            if let Some(r) = self.get_rect(img).copied() {
                if let Some(Some(tex)) = self.texture_ids.get(r.atlas_id as usize).copied() {
                    let (mut pw, mut ph) = (0, 0);
                    self.gl.BindTexture(gl::TEXTURE_2D, tex);
                    self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut pw);
                    self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut ph);
                    let mut iv = Vec::new();
                    quad(&mut iv, x, y, w, h, [r.x as f32, r.y as f32, r.w as f32, r.h as f32], [1.0; 4]);
                    draw(&self.gl, tex, (pw, ph), &iv);
                }
            }
        }
        self.gl.Disable(gl::BLEND);
        let (src, dst) = self.queue_render_state.blend_mode;
        self.gl.BlendFunc(src.into(), dst.into());
    }


    /// Presenta el búfer del juego en pantalla con escalado entero (o ajustado si no cabe ni a ×1)
    unsafe fn present_quad(&mut self, win_w: i32, win_h: i32, fb_w: i32, fb_h: i32) {
        let Some(u) = self.simple else { return };
        if fb_w <= 0 || fb_h <= 0 {
            return
        }
        let scale = (win_w / fb_w).min(win_h / fb_h);
        // escala entera si es al menos x2 (320x180 -> x3); si no (combate 576x324), ajustar a pantalla
        let (w, h) = if scale >= 2 {
            (fb_w * scale, fb_h * scale)
        } else if win_w * fb_h <= win_h * fb_w {
            (win_w, win_w * fb_h / fb_w)
        } else {
            (win_h * fb_w / fb_h, win_h)
        };
        let (x, y) = ((win_w - w) / 2, (win_h - h) / 2);
        if let Ok(mut r) = crate::render::PRESENT_RECT.lock() {
            *r = [x, y, w, h, fb_w, fb_h];
        }
        let mut fb_old = 0;
        self.gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fb_old);
        self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        self.gl.Disable(gl::SCISSOR_TEST);
        self.gl.Disable(gl::BLEND);
        self.gl.Viewport(0, 0, win_w, win_h);
        self.gl.ClearColor(0.0, 0.0, 0.0, 1.0);
        self.gl.Clear(gl::COLOR_BUFFER_BIT);
        self.gl.Viewport(x, win_h - y - h, w, h);
        self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.texture);
        let filter = if self.next_render_state.interpolate_pixels.into() { gl::LINEAR } else { gl::NEAREST };
        self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter as _);
        self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter as _);
        #[rustfmt::skip]
        let identity: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        self.gl.UniformMatrix4fv(u.model, 1, gl::FALSE, identity.as_ptr());
        self.gl.UniformMatrix4fv(u.viewproj, 1, gl::FALSE, identity.as_ptr());
        for loc in [u.colour_blending, u.repeat, u.lerp, u.alpha_test, u.fog_enabled] {
            self.gl.Uniform1f(loc, 0.0);
        }
        self.gl.Uniform2f(u.tex_size, fb_w as f32, fb_h as f32);
        let v = |px: f32, py: f32, tx: f32, ty: f32| Vertex {
            pos: [px, py, 0.0],
            tex_coord: [tx, ty],
            blend: [1.0; 4],
            atlas_xywh: [0.0, 0.0, fb_w as f32, fb_h as f32],
            normal: [0.0; 3],
        };
        // fila 0 de la textura (abajo en GL) = arriba de la pantalla, igual que el glBlitFramebuffer original
        let quad = [
            v(-1.0, 1.0, 0.0, 0.0),
            v(1.0, 1.0, 1.0, 0.0),
            v(-1.0, -1.0, 0.0, 1.0),
            v(1.0, 1.0, 1.0, 0.0),
            v(1.0, -1.0, 1.0, 1.0),
            v(-1.0, -1.0, 0.0, 1.0),
        ];
        self.gl.BufferData(gl::ARRAY_BUFFER, (size_of::<Vertex>() * 6) as _, quad.as_ptr().cast(), gl::STREAM_DRAW);
        self.gl.DrawArrays(gl::TRIANGLES, 0, 6);
        self.gl.Viewport(0, 0, win_w, win_h);
        self.draw_overlay(u, win_w, win_h);
        // restaurar el estado del juego
        if self.queue_render_state.alpha_blending {
            self.gl.Enable(gl::BLEND);
        }
        self.gl.Enable(gl::SCISSOR_TEST);
        self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, fb_old as u32);
        self.upload_state();
    }

    /// Sube el estado de dibujo al shader: bloque uniforme (original) o uniforms sueltos (simplificado)
    fn upload_state(&mut self) {
        let st = &self.queue_render_state;
        unsafe {
            match self.simple {
                None => {
                    self.gl.BindBuffer(gl::UNIFORM_BUFFER, self.buf_state);
                    self.gl.BufferData(
                        gl::UNIFORM_BUFFER,
                        offset_of!(RenderState, end_of_uniform) as _,
                        (st as *const RenderState).cast(),
                        gl::STATIC_DRAW,
                    );
                },
                Some(u) => {
                    let b = |v: GLBool| if bool::from(v) { 1.0f32 } else { 0.0 };
                    self.gl.UniformMatrix4fv(u.model, 1, gl::FALSE, st.model_matrix.as_ptr());
                    self.gl.UniformMatrix4fv(u.viewproj, 1, gl::FALSE, st.viewproj_matrix.as_ptr());
                    self.gl.Uniform1f(u.colour_blending, b(st.texture_blend));
                    self.gl.Uniform1f(u.repeat, b(st.texture_repeat));
                    self.gl.Uniform1f(u.lerp, b(st.interpolate_pixels));
                    self.gl.Uniform1f(u.alpha_test, b(st.depth_test));
                    self.gl.Uniform1f(u.fog_enabled, b(st.fog_enabled));
                    self.gl.Uniform1f(u.fog_begin, st.fog_begin);
                    self.gl.Uniform1f(u.fog_end, st.fog_end);
                    self.gl.Uniform4fv(u.fog_colour, 1, st.fog_colour.as_ptr());
                },
            }
        }
    }

    fn setup_frame(&mut self, clear_colour: Colour) {
        unsafe {
            // get framebuffer size
            let (mut width, mut height) = (0, 0);
            self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.texture);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);
            // set view
            self.set_view(0, 0, width, height, 0.0, 0, 0, width, height);
            // clear screen
            self.clear_view(clear_colour, 1.0);
        }
    }

    fn get_rect_mut(&mut self, id: AtlasRef) -> Option<&mut AtlasRect> {
        id.0.try_into()
            .ok()
            .and_then(move |id: usize| self.texture_rects.get_mut(id))
            .and_then(|o: &mut Option<AtlasRect>| o.as_mut())
    }

    /// Updates the renderer state.
    /// Call this before doing any drawing.
    fn update_render_state(&mut self) {
        if self.render_state_updated && self.queue_render_state != self.next_render_state {
            self.flush_queue();
            // update the viewproj matrix before doing any cloning
            if self.queue_render_state.view_matrix != self.next_render_state.view_matrix
                || self.queue_render_state.proj_matrix != self.next_render_state.proj_matrix
            {
                self.next_render_state.update_matrix(&self.gl);
            }

            let mut old_render_state = self.next_render_state.clone();
            std::mem::swap(&mut old_render_state, &mut self.queue_render_state);
            self.render_state_updated = false;
            let alpha_blending = self.next_render_state.alpha_blending;
            let blend_mode = self.next_render_state.blend_mode;
            let depth_test = self.next_render_state.depth_test;
            let write_depth = self.next_render_state.write_depth;
            let culling = self.next_render_state.culling;
            unsafe {
                if old_render_state.alpha_blending != alpha_blending {
                    if alpha_blending.into() {
                        self.gl.Enable(gl::BLEND);
                    } else {
                        self.gl.Disable(gl::BLEND);
                    }
                }
                if old_render_state.blend_mode != blend_mode {
                    let (src, dst) = blend_mode;
                    self.gl.BlendFunc(src.into(), dst.into());
                }
                if old_render_state.depth_test != depth_test {
                    if depth_test.into() {
                        self.gl.Enable(gl::DEPTH_TEST);
                    } else {
                        self.gl.Disable(gl::DEPTH_TEST);
                    }
                }
                if old_render_state.culling != culling {
                    if culling {
                        self.gl.Enable(gl::CULL_FACE);
                    } else {
                        self.gl.Disable(gl::CULL_FACE);
                    }
                }
                if old_render_state.write_depth != write_depth {
                    self.gl.DepthMask(write_depth as _);
                }

                self.upload_state();
                assert_eq!(self.gl.GetError(), 0);
            }
        }
    }

    fn setup_queue(&mut self, atlas_id: u32, queue_type: PrimitiveShape) {
        self.update_render_state();
        if atlas_id != self.current_atlas || self.queue_type != queue_type {
            self.flush_queue();
            self.current_atlas = atlas_id;
            self.queue_type = queue_type;
        }
    }

    fn push_primitive(&mut self, builder: &PrimitiveBuilder) {
        self.setup_queue(builder.get_atlas_id(), builder.get_shape());
        self.vertex_queue.extend_from_slice(builder.get_vertices());
    }

    fn draw_buffer(&mut self, atlas_id: u32, shape: PrimitiveShape, buffer: &[Vertex]) {
        if buffer.is_empty() {
            return
        }

        unsafe {
            // if something else broke check here just in case
            match self.gl.GetError() {
                0 => (),
                err => panic!("OpenGL threw an error somewhere (error code {})", err),
            }

            self.gl.BindTexture(gl::TEXTURE_2D, self.texture_ids[atlas_id as usize].unwrap());
            if let Some(u) = self.simple {
                let (mut w, mut h) = (0, 0);
                self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut w);
                self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut h);
                self.gl.Uniform2f(u.tex_size, w as f32, h as f32);
            }
            let filter_mode = if self.queue_render_state.interpolate_pixels.into() { gl::LINEAR } else { gl::NEAREST };
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter_mode as _);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter_mode as _);

            self.gl.BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<Vertex>() * buffer.len()) as _,
                buffer.as_ptr().cast(),
                gl::STREAM_DRAW,
            );
            assert_eq!(self.gl.GetError(), 0);

            self.gl.DrawArrays(shape.into(), 0, buffer.len() as i32);

            // https://www.khronos.org/opengl/wiki/Buffer_Object_Streaming#Buffer_re-specification
            // https://www.khronos.org/opengl/wiki/Buffer_Object#Invalidation
            self.gl.BufferData(
                gl::ARRAY_BUFFER,
                (size_of::<Vertex>() * buffer.len()) as _,
                ptr::null(),
                gl::STREAM_DRAW,
            );
        }
    }
}

impl RendererTrait for RendererImpl {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn max_texture_size(&self) -> u32 {
        let mut size: GLint = 0;
        unsafe {
            self.gl.GetIntegerv(gl::MAX_TEXTURE_SIZE, &mut size);
            if size == 16384 && CStr::from_ptr(self.gl.GetString(gl::VENDOR).cast()).to_bytes() == b"Intel" {
                // Intel driver bug throws GL_OUT_OF_MEMORY when allocating 16384x16384 texture
                // TODO: find proper fix or maybe allow allocating 16384x8192?
                size = 8192;
            }
            assert_eq!(self.gl.GetError(), 0);
        }
        size.max(0) as u32
    }

    fn push_atlases(&mut self, mut atl: AtlasBuilder) -> Result<(), String> {
        assert!(self.atlas_packers.is_empty(), "atlases should be initialized only once");

        let white_pixel_ref =
            atl.texture(1, 1, 0, 0, Box::new([0xFF, 0xFF, 0xFF, 0xFF])).ok_or("Couldn't pack white_pixel")?;
        let (packers, mut sprites) = atl.into_inner();
        self.white_pixel = sprites[white_pixel_ref.0 as usize].0;

        // update primitive buffers with white pixel
        self.reset_primitive_2d(PrimitiveType::PointList, None);
        self.reset_primitive_3d(PrimitiveType::PointList, None);

        unsafe {
            let textures: Vec<GLuint> = {
                let mut buf = vec![0 as GLuint; packers.len()];
                self.gl.GenTextures(buf.len() as _, buf.as_mut_ptr());
                for (i, (tex_id, packer)) in buf.iter().copied().zip(&packers).enumerate() {
                    let (width, height) = packer.size();

                    self.gl.BindTexture(gl::TEXTURE_2D, tex_id);
                    self.current_atlas = i as u32;

                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as _);
                    self.gl.TexImage2D(
                        gl::TEXTURE_2D,    // target
                        0,                 // level
                        gl::RGBA as _,     // internalformat
                        width as _,        // width
                        height as _,       // height
                        0,                 // border ("must be 0")
                        gl::BGRA,          // format
                        gl::UNSIGNED_BYTE, // type
                        ptr::null(),       // data
                    );
                }
                buf
            };

            // verify it actually worked
            match self.gl.GetError() {
                0 => (),
                err => return Err(format!("Failed to allocate texture on GPU! (OpenGL code {})", err)),
            }

            // upload textures
            for (atl_ref, pixels) in &sprites {
                if atl_ref.atlas_id == crate::render::atlas::LAZY_UNLOADED {
                    continue
                }
                if self.current_atlas != atl_ref.atlas_id {
                    self.gl.BindTexture(gl::TEXTURE_2D, textures[atl_ref.atlas_id as usize]);
                    self.current_atlas = atl_ref.atlas_id;
                }

                self.gl.TexSubImage2D(
                    gl::TEXTURE_2D,       // target
                    0,                    // level
                    atl_ref.x as _,       // xoffset
                    atl_ref.y as _,       // yoffset
                    atl_ref.w as _,       // width
                    atl_ref.h as _,       // height
                    gl::BGRA,             // format
                    gl::UNSIGNED_BYTE,    // type
                    pixels.as_ptr() as _, // pixels
                );
            }

            // verify it actually worked
            match self.gl.GetError() {
                0 => (),
                err => return Err(format!("Failed to upload textures to GPU! (OpenGL code {})", err)),
            }

            // generate framebuffers
            let mut fbo_ids = Vec::with_capacity(textures.len());
            for tex_id in &textures {
                let mut fbo = 0;
                self.gl.GenFramebuffers(1, &mut fbo);
                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, fbo);
                self.gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, *tex_id, 0);
                fbo_ids.push(Some(fbo));
            }
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer.fbo);

            // verify it actually worked
            match self.gl.GetError() {
                0 => (),
                err => return Err(format!("Failed to generate framebuffers! (OpenGL code {})", err)),
            }

            // store opengl texture handles
            self.texture_ids = textures.iter().map(|t| Some(*t)).collect();
            self.zbuf_ids.resize(self.texture_ids.len(), None);
            self.fbo_ids = fbo_ids;
            self.stock_atlas_count = textures.len() as u32 + 2; // TODO remove +2 when -o works
        }

        // store packers, discard pixeldata
        self.atlas_packers = packers;
        self.lazy.is_lazy = sprites.iter().map(|(ar, _)| ar.atlas_id == crate::render::atlas::LAZY_UNLOADED).collect();
        self.lazy.page_of = vec![u32::MAX; sprites.len()];
        if self.lazy.is_lazy.iter().any(|&l| l) {
            crate::startlog(&format!(
                "Texturas diferidas: {} (presupuesto {} MB)",
                self.lazy.is_lazy.iter().filter(|&&l| l).count(),
                self.lazy.budget_bytes / 1048576
            ));
        }
        self.texture_rects = sprites.drain(..).map(|(ar, _)| Some(ar)).collect();
        self.stock_texture_count = self.texture_rects.len();

        Ok(())
    }

    fn upload_sprite(
        &mut self,
        data: Box<[u8]>,
        width: i32,
        height: i32,
        origin_x: i32,
        origin_y: i32,
    ) -> Result<AtlasRef, String> {
        if self.dynamic_in_pages() && width > 0 && height > 0 && data.len() == (width * height * 4) as usize {
            let mut bgra = data;
            for px in bgra.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            return Ok(self.register_own(bgra, width, height, origin_x as f32 / width as f32, origin_y as f32 / height as f32))
        }
        let atlas_ref = self.create_surface(width, height, false)?;
        if let Some(rect) = self.get_rect_mut(atlas_ref) {
            rect.origin_x = origin_x as f32 / width as f32;
            rect.origin_y = origin_y as f32 / height as f32;
            let rect = self.get_rect(atlas_ref).unwrap();
            unsafe {
                // store previous
                let mut prev_tex2d = 0;
                self.gl.GetIntegerv(gl::TEXTURE_BINDING_2D, &mut prev_tex2d);

                // upload texture
                self.gl.BindTexture(gl::TEXTURE_2D, self.texture_ids[rect.atlas_id as usize].unwrap());
                self.gl.TexSubImage2D(
                    gl::TEXTURE_2D,     // target
                    0,                  // level
                    rect.x as _,        // xoffset
                    rect.y as _,        // yoffset
                    rect.w as _,        // width
                    rect.h as _,        // height
                    gl::RGBA,           // format
                    gl::UNSIGNED_BYTE,  // type
                    data.as_ptr() as _, // pixels
                );

                // verify it actually worked
                match self.gl.GetError() {
                    0 => (),
                    err => return Err(format!("Failed to upload texture to GPU! (OpenGL code {})", err)),
                }

                // cleanup
                self.gl.BindTexture(gl::TEXTURE_2D, prev_tex2d as _);
                assert_eq!(self.gl.GetError(), 0);
            }
        }
        Ok(atlas_ref)
    }

    fn duplicate_sprite(&mut self, atlas_ref: AtlasRef) -> Result<AtlasRef, String> {
        if self.dynamic_in_pages() {
            if let (Some(rect), true) = (
                self.get_rect(atlas_ref).copied(),
                usize::try_from(atlas_ref.0).ok().and_then(|i| self.lazy.is_lazy.get(i).copied()).unwrap_or(false),
            ) {
                if let Some(px) = self.lazy_pixels(atlas_ref) {
                    return Ok(self.register_own(px, rect.w, rect.h, rect.origin_x, rect.origin_y))
                }
            }
        }
        self.ensure(atlas_ref);
        if let Some(rect) = self.get_rect(atlas_ref).cloned() {
            let sprite = self.create_surface(rect.w, rect.h, false)?;
            let new_rect = self.get_rect_mut(sprite).unwrap();
            new_rect.origin_x = rect.origin_x;
            new_rect.origin_y = rect.origin_y;
            let new_rect = self.get_rect(sprite).unwrap();
            unsafe {
                // store previous
                let mut prev_read_fbo = 0;
                self.gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);
                let mut prev_tex2d = 0;
                self.gl.GetIntegerv(gl::TEXTURE_BINDING_2D, &mut prev_tex2d);

                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.fbo_ids[rect.atlas_id as usize].unwrap());
                self.gl.BindTexture(gl::TEXTURE_2D, self.texture_ids[new_rect.atlas_id as usize].unwrap());

                self.gl.CopyTexImage2D(gl::TEXTURE_2D, 0, gl::RGBA, rect.x, rect.y, rect.w as _, rect.h as _, 0);

                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, prev_read_fbo as _);
                self.gl.BindTexture(gl::TEXTURE_2D, prev_tex2d as _);

                // verify it actually worked
                match self.gl.GetError() {
                    0 => (),
                    err => return Err(format!("Failed to duplicate texture! (OpenGL code {})", err)),
                }
            }
            Ok(sprite)
        } else {
            Ok(AtlasRef(-1))
        }
    }

    fn delete_sprite(&mut self, atlas_ref: AtlasRef) {
        // this only deletes sprites created with upload_sprite
        self.flush_queue();
        if let Ok(i) = usize::try_from(atlas_ref.0) {
            if self.lazy.is_lazy.get(i).copied().unwrap_or(false) {
                // textura diferida: deja de existir, pero la página es compartida y no se borra
                self.lazy.is_lazy[i] = false;
                self.lazy.own.remove(&atlas_ref.0);
                if let Some(p) = self.lazy.page_of.get(i).copied().filter(|&p| p != u32::MAX) {
                    self.lazy.pages[p as usize].ids.retain(|&x| x != atlas_ref.0);
                    self.lazy.page_of[i] = u32::MAX;
                }
                self.texture_rects[i] = None;
                return
            }
        }
        if let Some(rect) = atlas_ref
            .0
            .try_into()
            .ok()
            .and_then(|id: usize| self.texture_rects.get_mut(id))
            .and_then(|o: &mut Option<AtlasRect>| o.take())
        {
            if rect.atlas_id >= self.stock_atlas_count {
                let tex_id = self.texture_ids[rect.atlas_id as usize].unwrap();
                unsafe {
                    self.gl.DeleteTextures(1, &tex_id);
                    self.texture_ids[rect.atlas_id as usize] = None;
                    if let Some(Some(zbuf)) = self.zbuf_ids.get(rect.atlas_id as usize) {
                        self.gl.DeleteTextures(1, zbuf);
                        self.zbuf_ids[rect.atlas_id as usize] = None;
                    }
                    if let Some(Some(fbo)) = self.fbo_ids.get(rect.atlas_id as usize) {
                        self.gl.DeleteFramebuffers(1, fbo);
                        self.fbo_ids[rect.atlas_id as usize] = None;
                    }
                    assert_eq!(self.gl.GetError(), 0);
                }
            }
        }
    }

    fn set_vsync(&self, vsync: bool) {
        unsafe { self.imp.set_swap_interval(if vsync { 1 } else { 0 }) };
    }

    fn get_vsync(&self) -> bool {
        unsafe { self.imp.get_swap_interval() != 0 }
    }

    fn wait_vsync(&self) {
        unsafe { self.imp.wait_vsync() }
    }

    fn create_sprite_colour(&mut self, width: i32, height: i32, col: Colour) -> Result<AtlasRef, String> {
        if self.dynamic_in_pages() && width > 0 && height > 0 {
            let px = [(col.b * 255.0) as u8, (col.g * 255.0) as u8, (col.r * 255.0) as u8, 255u8];
            let data: Box<[u8]> = px.iter().copied().cycle().take((width * height * 4) as usize).collect();
            return Ok(self.register_own(data, width, height, 0.0, 0.0))
        }
        let atlas_ref = self.create_surface(width, height, false)?;
        if let Some(rect) = self.get_rect(atlas_ref) {
            unsafe {
                let mut prev_read_fbo = 0;
                self.gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);
                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.fbo_ids[rect.atlas_id as usize].unwrap());
                self.gl.ClearColor(col.r as f32, col.g as f32, col.b as f32, 1.0);
                self.gl.Clear(gl::COLOR_BUFFER_BIT);
                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, prev_read_fbo as _);
            }
        }
        Ok(atlas_ref)
    }

    fn create_surface(&mut self, width: i32, height: i32, has_zbuffer: bool) -> Result<AtlasRef, String> {
        let atlas_id = if let Some(id) = self.texture_ids.iter().position(|x| x.is_none()) {
            id as u32
        } else {
            self.texture_ids.push(None);
            self.zbuf_ids.push(None);
            self.fbo_ids.push(None);
            self.texture_ids.len() as u32 - 1
        };
        unsafe {
            // store previous
            let mut prev_tex2d = 0;
            self.gl.GetIntegerv(gl::TEXTURE_BINDING_2D, &mut prev_tex2d);
            let mut prev_fbo = 0;
            self.gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut prev_fbo);

            // generate new
            let mut tex_id: GLuint = 0;
            self.gl.GenTextures(1, &mut tex_id);
            self.gl.BindTexture(gl::TEXTURE_2D, tex_id);

            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as _);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as _);

            self.gl.TexImage2D(
                gl::TEXTURE_2D,    // target
                0,                 // level
                gl::RGBA as _,     // internalformat
                width as _,        // width
                height as _,       // height
                0,                 // border ("must be 0")
                gl::RGBA,          // format
                gl::UNSIGNED_BYTE, // type
                ptr::null(),       // data
            );

            // verify it actually worked
            match self.gl.GetError() {
                0 => (),
                err => return Err(format!("Failed to allocate texture on GPU! (OpenGL code {})", err)),
            }

            // generate fbo
            let mut fbo = 0;
            self.gl.GenFramebuffers(1, &mut fbo);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, fbo);
            self.gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex_id, 0);
            match self.gl.GetError() {
                0 => (),
                err => return Err(format!("Failed to generate framebuffer! (OpenGL code {})", err)),
            }

            // generate zbuffer if applicable
            if has_zbuffer {
                let mut zbuf_id: GLuint = 0;
                self.gl.GenTextures(1, &mut zbuf_id);
                self.gl.BindTexture(gl::TEXTURE_2D, zbuf_id);
                self.gl.TexImage2D(
                    gl::TEXTURE_2D,      // target
                    0,                   // level
                    self.zbuf_format,    // internalformat
                    width as _,          // width
                    height as _,         // height
                    0,                   // border ("must be 0")
                    gl::DEPTH_COMPONENT, // format
                    gl::FLOAT,           // type
                    ptr::null(),         // data
                );
                self.gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, zbuf_id, 0);
                match self.gl.GetError() {
                    0 => (),
                    err => return Err(format!("Failed to generate depth buffer! (OpenGL code {})", err)),
                }
                // store handle
                self.zbuf_ids[atlas_id as usize] = Some(zbuf_id);
            }

            // store opengl texture handles
            self.texture_ids[atlas_id as usize] = Some(tex_id);
            self.fbo_ids[atlas_id as usize] = Some(fbo);

            // cleanup
            self.gl.BindTexture(gl::TEXTURE_2D, prev_tex2d as _);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, prev_fbo as _);
            assert_eq!(self.gl.GetError(), 0);
        }
        let id = self.texture_rects.len() as i32;
        self.texture_rects.push(Some(AtlasRect {
            atlas_id,
            x: 0,
            y: 0,
            w: width,
            h: height,
            origin_x: 0.0,
            origin_y: 0.0,
        }));
        Ok(AtlasRef(id))
    }

    fn set_target(&mut self, atlas_ref: AtlasRef) {
        self.flush_queue();
        if let Some((rect, Some(Some(fbo_id)))) =
            self.get_rect(atlas_ref).map(|rect| (rect, self.fbo_ids.get(rect.atlas_id as usize)))
        {
            let AtlasRect { x, y, w, h, .. } = *rect;
            unsafe {
                self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, *fbo_id);
                // set viewport here since set_view doesn't
                self.gl.Viewport(x, y, w, h);
                self.gl.Scissor(x, y, w, h);
                assert_eq!(self.gl.GetError(), 0);
            }
            self.set_view(x, y, w, h, 0.0, x, y, w, h);
        }
    }

    fn reset_target(&mut self) {
        self.flush_queue();
        unsafe {
            self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.framebuffer.fbo);

            // reset view
            let (mut fb_width, mut fb_height) = (0, 0);
            self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.texture);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut fb_width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut fb_height);
            assert_eq!(self.gl.GetError(), 0);
            self.set_view(0, 0, fb_width, fb_height, 0.0, 0, 0, fb_width, fb_height);
        }
    }

    fn copy_surface(
        &mut self,
        dest: AtlasRef,
        mut dest_x: i32,
        mut dest_y: i32,
        src: AtlasRef,
        mut src_x: i32,
        mut src_y: i32,
        mut width: i32,
        mut height: i32,
    ) {
        let (src_rect, dest_rect) = match (self.get_rect(src), self.get_rect(dest)) {
            (Some(src), Some(dest)) => (src, dest),
            _ => return,
        };
        // correct coordinates
        if src_x < 0 {
            dest_x -= src_x;
            width += src_x;
            src_x = 0;
        }
        if src_y < 0 {
            dest_y -= src_y;
            height += src_y;
            src_y = 0;
        }
        if src_x + width > src_rect.w {
            width = src_rect.w - src_x;
        }
        if src_y + height > src_rect.h {
            height = dest_rect.h - src_y;
        }
        if dest_x < 0 {
            src_x -= dest_x;
            width += dest_x;
            dest_x = 0;
        }
        if dest_y < 0 {
            src_y -= dest_y;
            height += dest_y;
            dest_y = 0;
        }
        if dest_x + width > dest_rect.w {
            width = dest_rect.w - dest_x;
        }
        if dest_y + height > dest_rect.h {
            height = dest_rect.h - dest_y;
        }
        if width > 0 && height > 0 {
            // actual copy
            if let (Some(Some(src_id)), Some(Some(dst_id))) =
                (self.fbo_ids.get(src_rect.atlas_id as usize), self.fbo_ids.get(dest_rect.atlas_id as usize))
            {
                unsafe {
                    // On Intel, glBlitFrameBuffer just does nothing if the scissor box is too big, which it
                    // very well could be. So just disable the scissor test for now.
                    self.gl.Disable(gl::SCISSOR_TEST);

                    // Remember old framebuffer so we can rebind it after we're done
                    let mut fb_old = 0;
                    self.gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fb_old);
                    assert_eq!(self.gl.GetError(), 0);

                    self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, *dst_id);
                    self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, *src_id);
                    self.gl.BlitFramebuffer(
                        src_x,
                        src_y,
                        src_x + width,
                        src_y + height,
                        dest_x,
                        dest_y,
                        dest_x + width,
                        dest_y + height,
                        gl::COLOR_BUFFER_BIT,
                        gl::NEAREST,
                    );
                    self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, fb_old as u32);

                    self.gl.Enable(gl::SCISSOR_TEST);

                    assert_eq!(self.gl.GetError(), 0);
                }
            }
        }
    }

    fn set_zbuf_trashed(&mut self, trashed: bool) {
        if trashed != self.zbuf_trashed {
            self.flush_queue();
            self.zbuf_trashed = trashed;
            unsafe {
                let mut prev_fbo = 0;
                self.gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut prev_fbo);
                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer.fbo);
                assert_eq!(self.gl.GetError(), 0);
                self.gl.FramebufferTexture2D(
                    gl::READ_FRAMEBUFFER,
                    gl::DEPTH_ATTACHMENT,
                    gl::TEXTURE_2D,
                    if trashed { 0 } else { self.framebuffer.zbuf },
                    0,
                );
                assert_eq!(self.gl.GetError(), 0);
                self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, prev_fbo as _);
            }
        }
    }

    fn get_zbuf_trashed(&self) -> bool {
        self.zbuf_trashed
    }

    fn resize_framebuffer(&mut self, width: u32, height: u32, store: bool) {
        self.flush_queue();
        unsafe {
            // get old size
            let old_tex = self.framebuffer.texture;
            self.gl.BindTexture(gl::TEXTURE_2D, old_tex);
            let (mut old_width, mut old_height) = (0, 0);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut old_width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut old_height);
            // set up new texture
            self.gl.GenTextures(1, &mut self.framebuffer.texture);
            self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.texture);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            self.gl.TexImage2D(
                gl::TEXTURE_2D,    // target
                0,                 // level
                gl::RGBA as _,     // internalformat
                width as _,        // width
                height as _,       // height
                0,                 // border ("must be 0")
                gl::RGBA,          // format
                gl::UNSIGNED_BYTE, // type
                ptr::null(),       // data
            );
            assert_eq!(self.gl.GetError(), 0);
            // set up new zbuffer
            let old_zbuf = self.framebuffer.zbuf;
            self.gl.GenTextures(1, &mut self.framebuffer.zbuf);
            self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.zbuf);
            self.gl.TexImage2D(
                gl::TEXTURE_2D,      // target
                0,                   // level
                self.zbuf_format,    // internalformat
                width as _,          // width
                height as _,         // height
                0,                   // border ("must be 0")
                gl::DEPTH_COMPONENT, // format
                gl::FLOAT,           // type
                ptr::null(),         // data
            );
            assert_eq!(self.gl.GetError(), 0);
            // set up new fbo
            let old_fbo = self.framebuffer.fbo;
            self.gl.GenFramebuffers(1, &mut self.framebuffer.fbo);
            self.gl.BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer.fbo);
            self.gl.FramebufferTexture2D(
                gl::READ_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.framebuffer.texture,
                0,
            );
            self.gl.FramebufferTexture2D(
                gl::READ_FRAMEBUFFER,
                gl::DEPTH_ATTACHMENT,
                gl::TEXTURE_2D,
                self.framebuffer.zbuf,
                0,
            );
            assert_eq!(self.gl.GetError(), 0);
            // draw old fb onto new
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, old_fbo);
            let copy_width = width.min(old_width as _);
            let copy_height = height.min(old_height as _);
            self.gl.BlitFramebuffer(
                0,
                0,
                copy_width as _,
                copy_height as _,
                0,
                0,
                copy_width as _,
                copy_height as _,
                gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT,
                gl::NEAREST,
            );
            assert_eq!(self.gl.GetError(), 0);
            if store {
                // store old texture and fbo, delete previous stored one
                if let Some(framebuffer) = &self.stored_framebuffer {
                    self.gl.DeleteTextures(1, &framebuffer.texture);
                    self.gl.DeleteTextures(1, &framebuffer.zbuf);
                    self.gl.DeleteFramebuffers(1, &framebuffer.fbo);
                }
                self.stored_framebuffer = Some(Framebuffer { texture: old_tex, zbuf: old_zbuf, fbo: old_fbo });
            } else {
                // delete old texture and fbo
                self.gl.DeleteTextures(1, &old_tex);
                self.gl.DeleteTextures(1, &old_zbuf);
                self.gl.DeleteFramebuffers(1, &old_fbo);
            }
            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn get_texture_id(&mut self, atl_ref: AtlasRef) -> i32 {
        atl_ref.0
    }

    fn get_texture_from_id(&self, id: i32) -> Option<AtlasRef> {
        Some(AtlasRef(id))
    }

    fn get_texture_rects(&self) -> Vec<Option<AtlasRect>> {
        self.texture_rects[self.stock_texture_count..].to_vec()
    }

    fn set_texture_rects(&mut self, rects: &[Option<AtlasRect>]) {
        self.texture_rects.truncate(self.stock_texture_count);
        self.texture_rects.extend_from_slice(rects);
    }

    fn dump_sprite_part(&self, atlas_ref: AtlasRef, part_x: i32, part_y: i32, part_w: i32, part_h: i32) -> Box<[u8]> {
        let rect = match self.get_rect(atlas_ref) {
            Some(rect) => {
                AtlasRect { x: rect.x + part_x, y: rect.y + part_y, w: part_w, h: part_h, ..*rect }
            },
            None => return Box::new([]),
        };
        unsafe {
            // store read fbo
            let mut prev_read_fbo: GLint = 0;
            self.gl.GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut prev_read_fbo);

            // bind texture fbo
            self.gl.BindFramebuffer(
                gl::READ_FRAMEBUFFER,
                self.fbo_ids[rect.atlas_id as usize].expect("Trying to dump nonexistent sprite"),
            );

            // read data
            let len = (rect.w * rect.h * 4) as usize;
            let mut data: Vec<u8> = Vec::with_capacity(len);
            data.set_len(len);
            self.gl.ReadPixels(rect.x, rect.y, rect.w, rect.h, gl::RGBA, gl::UNSIGNED_BYTE, data.as_mut_ptr().cast());

            assert_eq!(self.gl.GetError(), 0);

            // cleanup
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, prev_read_fbo as GLuint);
            assert_eq!(self.gl.GetError(), 0);

            data.into_boxed_slice()
        }
    }

    fn get_pixels(&self, x: i32, y: i32, w: i32, h: i32) -> Box<[u8]> {
        unsafe {
            let len = (w * h * 4) as usize;
            let mut data: Vec<u8> = Vec::with_capacity(len);
            data.set_len(len);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer.fbo);
            self.gl.ReadPixels(x, y, w, h, gl::RGBA, gl::UNSIGNED_BYTE, data.as_mut_ptr().cast());
            assert_eq!(self.gl.GetError(), 0);
            data.into_boxed_slice()
        }
    }

    fn stored_pixels(&self) -> Box<[u8]> {
        let fb = self.stored_framebuffer.unwrap_or(self.framebuffer);
        unsafe {
            let (width, height) = self.stored_size();
            let len = (width * height * 4) as usize;
            let mut data: Vec<u8> = Vec::with_capacity(len);
            data.set_len(len);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, fb.fbo);
            self.gl.ReadPixels(0, 0, width as _, height as _, gl::RGBA, gl::UNSIGNED_BYTE, data.as_mut_ptr().cast());
            assert_eq!(self.gl.GetError(), 0);
            data.into_boxed_slice()
        }
    }

    fn stored_zbuffer(&self) -> Box<[f32]> {
        let fb = self.stored_framebuffer.unwrap_or(self.framebuffer);
        unsafe {
            self.gl.BindTexture(gl::TEXTURE_2D, fb.zbuf);
            let mut width = 0;
            let mut height = 0;
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);
            let len = (width * height) as usize;
            let mut data: Vec<f32> = Vec::with_capacity(len);
            data.set_len(len);
            self.gl.GetTexImage(gl::TEXTURE_2D, 0, gl::DEPTH_COMPONENT, gl::FLOAT, data.as_mut_ptr().cast());
            data.into_boxed_slice()
        }
    }

    fn set_stored(&mut self, rgba: Box<[u8]>, zbuf: Box<[f32]>, fb_w: u32, fb_h: u32) {
        unsafe {
            let mut fbo = 0;
            let mut texture = 0;
            let mut zbuffer = 0;
            self.gl.GenFramebuffers(1, &mut fbo);
            self.gl.GenTextures(1, &mut texture);
            self.gl.GenTextures(1, &mut zbuffer);

            assert_eq!(self.gl.GetError(), 0);

            self.gl.BindTexture(gl::TEXTURE_2D, texture);
            assert_eq!(self.gl.GetError(), 0);
            self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as _);
            assert_eq!(self.gl.GetError(), 0);
            self.gl.TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as _,
                fb_w as _,
                fb_h as _,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                rgba.as_ptr().cast(),
            );

            assert_eq!(self.gl.GetError(), 0);

            self.gl.BindTexture(gl::TEXTURE_2D, zbuffer);
            self.gl.TexImage2D(
                gl::TEXTURE_2D,
                0,
                self.zbuf_format,
                fb_w as _,
                fb_h as _,
                0,
                gl::DEPTH_COMPONENT,
                gl::FLOAT,
                zbuf.as_ptr().cast(),
            );

            assert_eq!(self.gl.GetError(), 0);

            self.gl.BindFramebuffer(gl::FRAMEBUFFER, fbo);
            self.gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture, 0);
            assert_eq!(self.gl.GetError(), 0);
            self.gl.FramebufferTexture2D(gl::READ_FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::TEXTURE_2D, zbuffer, 0);

            if let Some(framebuffer) = &self.stored_framebuffer {
                self.gl.DeleteTextures(1, &framebuffer.texture);
                self.gl.DeleteTextures(1, &framebuffer.zbuf);
                self.gl.DeleteFramebuffers(1, &framebuffer.fbo);
            }

            self.stored_framebuffer = Some(Framebuffer { fbo, texture, zbuf: zbuffer });

            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn dump_dynamic_textures(&self) -> Vec<Option<SavedTexture>> {
        unsafe {
            // store previous
            let mut prev_tex2d = 0;
            self.gl.GetIntegerv(gl::TEXTURE_BINDING_2D, &mut prev_tex2d);

            let mut textures = Vec::with_capacity(self.texture_ids.len() - self.stock_atlas_count as usize);
            for (tex_id, zbuf_id) in self
                .texture_ids
                .iter()
                .copied()
                .zip(self.zbuf_ids.iter().copied())
                .skip(self.stock_atlas_count as usize)
            {
                textures.push(match tex_id {
                    Some(tex_id) => {
                        self.gl.BindTexture(gl::TEXTURE_2D, tex_id);
                        let mut width = 0;
                        let mut height = 0;
                        self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
                        self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);
                        let len = (width * height) as usize;
                        let mut pixels: Vec<u8> = Vec::with_capacity(len * 4);
                        pixels.set_len(len * 4);
                        self.gl.GetTexImage(gl::TEXTURE_2D, 0, gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_mut_ptr().cast());
                        let zbuf = if let Some(zbuf_id) = zbuf_id {
                            self.gl.BindTexture(gl::TEXTURE_2D, zbuf_id);
                            let mut zbuf: Vec<f32> = Vec::with_capacity(len);
                            zbuf.set_len(len);
                            self.gl.GetTexImage(
                                gl::TEXTURE_2D,
                                0,
                                gl::DEPTH_COMPONENT,
                                gl::FLOAT,
                                zbuf.as_mut_ptr().cast(),
                            );
                            Some(zbuf.into_boxed_slice())
                        } else {
                            None
                        };
                        Some(SavedTexture { width, height, pixels: pixels.into_boxed_slice(), zbuf })
                    },
                    None => None,
                });
            }

            self.gl.BindTexture(gl::TEXTURE_2D, prev_tex2d as _);
            assert_eq!(self.gl.GetError(), 0);

            textures
        }
    }

    fn upload_dynamic_textures(&mut self, textures: &[Option<SavedTexture>]) {
        unsafe {
            for tex_id in self.texture_ids.iter_mut().skip(self.stock_atlas_count as usize) {
                if let Some(tex_id) = tex_id.as_ref() {
                    self.gl.DeleteTextures(1, tex_id);
                }
                *tex_id = None;
            }
            self.texture_ids.resize(self.stock_atlas_count as usize + textures.len(), None);
            for fbo_id in self.fbo_ids.iter_mut().skip(self.stock_atlas_count as usize) {
                if let Some(fbo_id) = fbo_id.as_ref() {
                    self.gl.DeleteFramebuffers(1, fbo_id);
                }
                *fbo_id = None;
            }
            self.fbo_ids.resize(self.stock_atlas_count as usize + textures.len(), None);
            for tex_id in self.zbuf_ids.iter_mut().skip(self.stock_atlas_count as usize) {
                if let Some(tex_id) = tex_id.as_ref() {
                    self.gl.DeleteTextures(1, tex_id);
                }
                *tex_id = None;
            }
            self.zbuf_ids.resize(self.stock_atlas_count as usize + textures.len(), None);
            for (i, tex) in textures.iter().enumerate() {
                let i = i + self.stock_atlas_count as usize;
                if let Some(tex) = tex.as_ref() {
                    let mut tex_id = 0;
                    self.gl.GenTextures(1, &mut tex_id);
                    self.gl.BindTexture(gl::TEXTURE_2D, tex_id);

                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as _);
                    self.gl.TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as _);

                    self.gl.TexImage2D(
                        gl::TEXTURE_2D,
                        0,
                        gl::RGBA as _,
                        tex.width,
                        tex.height,
                        0,
                        gl::RGBA,
                        gl::UNSIGNED_BYTE,
                        tex.pixels.as_ptr().cast(),
                    );
                    self.texture_ids[i] = Some(tex_id);

                    let mut fbo_id = 0;
                    self.gl.GenFramebuffers(1, &mut fbo_id);
                    self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, fbo_id);
                    self.gl.FramebufferTexture2D(
                        gl::READ_FRAMEBUFFER,
                        gl::COLOR_ATTACHMENT0,
                        gl::TEXTURE_2D,
                        tex_id,
                        0,
                    );
                    self.fbo_ids[i] = Some(fbo_id);

                    if let Some(zbuf) = tex.zbuf.as_ref() {
                        let mut zbuf_id = 0;
                        self.gl.GenTextures(1, &mut zbuf_id);
                        self.gl.BindTexture(gl::TEXTURE_2D, zbuf_id);
                        self.gl.TexImage2D(
                            gl::TEXTURE_2D,
                            0,
                            self.zbuf_format,
                            tex.width,
                            tex.height,
                            0,
                            gl::DEPTH_COMPONENT,
                            gl::FLOAT,
                            zbuf.as_ptr().cast(),
                        );
                        self.zbuf_ids[i] = Some(zbuf_id);
                        self.gl.FramebufferTexture2D(
                            gl::READ_FRAMEBUFFER,
                            gl::DEPTH_ATTACHMENT,
                            gl::TEXTURE_2D,
                            zbuf_id,
                            0,
                        );
                    }
                }
            }
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer.fbo);
            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn get_rect(&self, id: AtlasRef) -> Option<&AtlasRect> {
        id.0.try_into()
            .ok()
            .and_then(|id: usize| self.texture_rects.get(id))
            .and_then(|o: &Option<AtlasRect>| o.as_ref())
    }

    fn set_sprite_origin(&mut self, id: AtlasRef, origin_x: i32, origin_y: i32) {
        self.flush_queue();
        if let Some(r) = self.get_rect_mut(id) {
            if r.w > 0 && r.h > 0 {
                r.origin_x = origin_x as f32 / r.w as f32;
                r.origin_y = origin_y as f32 / r.h as f32;
            }
        }
    }

    fn set_lazy_provider(&mut self, provider: crate::render::LazyProvider) {
        self.lazy.provider = Some(provider);
    }

    fn draw_sprite_general(
        &mut self,
        texture: AtlasRef,
        part_x: f64,
        part_y: f64,
        part_w: f64,
        part_h: f64,
        x: f64,
        y: f64,
        xscale: f64,
        yscale: f64,
        angle: f64,
        col1: i32,
        col2: i32,
        col3: i32,
        col4: i32,
        alpha: f64,
        use_origin: bool,
    ) {
        self.ensure(texture);
        let atlas_ref = match self.get_rect(texture) {
            Some(rect) => *rect,
            None => return,
        };

        self.set_texture_repeat(false);

        // get angle
        let angle = -angle.to_radians();
        let angle_sin = angle.sin();
        let angle_cos = angle.cos();

        // get real width of drawn sprite
        let width: f64 = xscale * f64::from(part_w);
        let height: f64 = yscale * f64::from(part_h);
        // calculate pre-rotation corner offsets from sprite origin
        // incl. subtraction 0.5 from left and top (GM does this in an attempt to combat the DX half-pixel offset)
        let (left, top): (f64, f64) = if use_origin {
            (-width * f64::from(atlas_ref.origin_x) - 0.5, -height * f64::from(atlas_ref.origin_y) - 0.5)
        } else {
            (-0.5, -0.5)
        };
        let right: f64 = left + width;
        let bottom: f64 = top + height;

        // get texture corners
        let tex_left = f64::from(part_x) / f64::from(atlas_ref.w);
        let tex_top = f64::from(part_y) / f64::from(atlas_ref.h);
        let tex_right = tex_left + f64::from(part_w) / f64::from(atlas_ref.w);
        let tex_bottom = tex_top + f64::from(part_h) / f64::from(atlas_ref.h);

        let (tex_left, tex_top, tex_right, tex_bottom) =
            (tex_left as f32, tex_top as f32, tex_right as f32, tex_bottom as f32);

        let normal = [0.0, 0.0, 0.0];
        let depth = self.depth;

        // rotate around draw origin
        let rotate = |xoff, yoff| {
            [(x + xoff * angle_cos - yoff * angle_sin) as f32, (y + yoff * angle_cos + xoff * angle_sin) as f32, depth]
        };

        // push the vertices
        self.push_primitive(
            PrimitiveBuilder::new(atlas_ref, PrimitiveType::TriFan, true)
                .push_vertex(rotate(left, top), [tex_left, tex_top], split_colour(col1, alpha), normal)
                .push_vertex(rotate(right, top), [tex_right, tex_top], split_colour(col2, alpha), normal)
                .push_vertex(rotate(right, bottom), [tex_right, tex_bottom], split_colour(col3, alpha), normal)
                .push_vertex(rotate(left, bottom), [tex_left, tex_bottom], split_colour(col4, alpha), normal),
        );
    }

    fn draw_sprite_pos(
        &mut self,
        texture: AtlasRef,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
        x4: f64,
        y4: f64,
        alpha: f64,
    ) {
        self.ensure(texture);
        let atlas_ref = match self.get_rect(texture) {
            Some(rect) => *rect,
            None => return,
        };

        self.set_texture_repeat(false);

        // get texture corners
        let tex_left = 0.0;
        let tex_top = 0.0;
        let tex_right = tex_left + 1.0;
        let tex_bottom = tex_top + 1.0;

        let (tex_left, tex_top, tex_right, tex_bottom) =
            (tex_left as f32, tex_top as f32, tex_right as f32, tex_bottom as f32);

        let normal = [0.0, 0.0, 0.0];
        let depth = self.depth;

        // correct for gm offset
        let correct = |xoff, yoff| {
            [(xoff-0.5) as f32, (yoff-0.5) as f32, depth]
        };

        // push the vertices
        self.push_primitive(
            PrimitiveBuilder::new(atlas_ref, PrimitiveType::TriFan, true)
                .push_vertex(correct(x1, y1), [tex_left, tex_top], split_colour(0xffffff, alpha), normal)
                .push_vertex(correct(x2, y2), [tex_right, tex_top], split_colour(0xffffff, alpha), normal)
                .push_vertex(correct(x3, y3), [tex_right, tex_bottom], split_colour(0xffffff, alpha), normal)
                .push_vertex(correct(x4, y4), [tex_left, tex_bottom], split_colour(0xffffff, alpha), normal),
        );
    }

    fn draw_rectangle(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: i32, alpha: f64) {
        let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
        let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
        let x2 = if x2 == x2.floor() { x2 + 0.01 } else { x2 };
        let y2 = if y2 == y2.floor() { y2 + 0.01 } else { y2 };
        self.push_primitive(
            ShapeBuilder::new(false, self.white_pixel, alpha, self.depth)
                .push_point(x1, y1, colour)
                .push_point(x2, y1, colour)
                .push_point(x2, y2, colour)
                .push_point(x1, y2, colour)
                .build(),
        );
    }

    fn draw_rectangle_outline(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, colour: i32, alpha: f64) {
        let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
        let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
        let x2 = if x2 == x2.floor() { x2 + 0.01 } else { x2 };
        let y2 = if y2 == y2.floor() { y2 + 0.01 } else { y2 };
        self.push_primitive(
            ShapeBuilder::new(true, self.white_pixel, alpha, self.depth)
                .push_point(x1, y1, colour)
                .push_point(x2, y1, colour)
                .push_point(x2, y2, colour)
                .push_point(x1, y2, colour)
                .build(),
        );
    }

    fn draw_rectangle_gradient(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        c1: i32,
        c2: i32,
        c3: i32,
        c4: i32,
        alpha: f64,
        outline: bool,
    ) {
        let (x1, x2) = if x2 < x1 { (x2, x1) } else { (x1, x2) };
        let (y1, y2) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
        let x2 = if x2 == x2.floor() { x2 + 0.01 } else { x2 };
        let y2 = if y2 == y2.floor() { y2 + 0.01 } else { y2 };
        self.push_primitive(
            ShapeBuilder::new(outline, self.white_pixel, alpha, self.depth)
                .push_point(x1, y1, c1)
                .push_point(x2, y1, c2)
                .push_point(x2, y2, c3)
                .push_point(x1, y2, c4)
                .build(),
        );
    }

    fn draw_point(&mut self, x: f64, y: f64, colour: i32, alpha: f64) {
        self.setup_queue(self.white_pixel.atlas_id, PrimitiveShape::Point);
        self.vertex_queue.push(Vertex {
            pos: [x as f32, y as f32, self.depth],
            tex_coord: [crate::render::UNTEXTURED; 2],
            blend: split_colour(colour, alpha),
            atlas_xywh: self.white_pixel.into(),
            normal: [0.0, 0.0, 0.0],
        });
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, width: Option<f64>, c1: i32, c2: i32, alpha: f64) {
        if let Some(width) = width {
            let length = (x2 - x1).hypot(y2 - y1);
            // on the off chance that they're in different points but the length is still somehow 0, check length
            if length != 0.0 {
                // calculate corners
                let width_x = (y2 - y1) * (width / 2.0) / length;
                let width_y = (x2 - x1) * (width / 2.0) / length;
                // actually push the rectangle
                self.push_primitive(
                    ShapeBuilder::new(false, self.white_pixel, alpha, self.depth)
                        .push_point(x1 - width_x, y1 + width_y, c1)
                        .push_point(x1 + width_x, y1 - width_y, c1)
                        .push_point(x2 + width_x, y2 - width_y, c2)
                        .push_point(x2 - width_x, y2 + width_y, c2)
                        .build(),
                );
            }
        } else {
            self.push_primitive(
                ShapeBuilder::new(true, self.white_pixel, alpha, self.depth)
                    .push_point(x1, y1, c1)
                    .push_point(x2, y2, c2)
                    .build(),
            );
        }
    }

    fn draw_triangle(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x3: f64,
        y3: f64,
        c1: i32,
        c2: i32,
        c3: i32,
        alpha: f64,
        outline: bool,
    ) {
        self.push_primitive(
            ShapeBuilder::new(outline, self.white_pixel, alpha, self.depth)
                .push_point(x1, y1, c1)
                .push_point(x2, y2, c2)
                .push_point(x3, y3, c3)
                .build(),
        );
    }

    fn draw_ellipse(&mut self, x: f64, y: f64, rad_x: f64, rad_y: f64, c1: i32, c2: i32, alpha: f64, outline: bool) {
        let mut builder = ShapeBuilder::new(outline, self.white_pixel, alpha, self.depth);
        if !outline {
            builder.push_point(x, y, c1);
        }
        for i in 0..=self.circle_precision {
            let angle = f64::from(i) * 2.0 * PI / f64::from(self.circle_precision);
            builder.push_point(x + rad_x * angle.cos(), y + rad_y * angle.sin(), c2);
        }
        self.push_primitive(builder.build());
    }

    fn draw_roundrect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, c1: i32, c2: i32, alpha: f64, outline: bool) {
        let x2 = if x2 == x2.floor() { x2 + 0.01 } else { x2 };
        let y2 = if y2 == y2.floor() { y2 + 0.01 } else { y2 };
        let xcenter = (x1 + x2) / 2.0;
        let ycenter = (y1 + y2) / 2.0;
        let width = (x2 - x1).abs();
        let height = (y2 - y1).abs();
        let rad_x = width.min(10.0) / 2.0;
        let rad_y = height.min(10.0) / 2.0;
        let rect_half_w = (width / 2.0 - rad_x).max(0.0);
        let rect_half_h = (height / 2.0 - rad_y).max(0.0);
        let mut builder = ShapeBuilder::new(outline, self.white_pixel, alpha, self.depth);
        if !outline {
            builder.push_point(xcenter, ycenter, c1);
        }
        let quarter_circle = self.circle_precision / 4;
        for quad in 0..4 {
            let circle_x = xcenter + if quad == 0 || quad == 3 { rect_half_w } else { -rect_half_w };
            let circle_y = ycenter + if quad < 2 { rect_half_h } else { -rect_half_h };
            for i in quarter_circle * quad..=quarter_circle * (quad + 1) {
                let angle = f64::from(i) * 2.0 * PI / f64::from(self.circle_precision);
                builder.push_point(circle_x + rad_x * angle.cos(), circle_y + rad_y * angle.sin(), c2);
            }
        }
        self.push_primitive(builder.push_point(xcenter + rect_half_w + rad_x, ycenter + rect_half_h, c2).build());
    }

    fn set_circle_precision(&mut self, prec: i32) {
        self.circle_precision = (prec.max(4).min(64) >> 2) << 2;
    }

    fn get_circle_precision(&self) -> i32 {
        self.circle_precision
    }

    fn reset_primitive_2d(&mut self, ptype: PrimitiveType, atlas_ref: Option<AtlasRef>) {
        if let Some(a) = atlas_ref {
            self.ensure(a);
        }
        let ar = atlas_ref.and_then(|ar| self.get_rect(ar).copied());
        self.primitive_2d = PrimitiveBuilder::new(
            ar.unwrap_or(self.white_pixel),
            ptype,
            ar.is_some(),
        );
    }

    fn vertex_2d(&mut self, x: f64, y: f64, xtex: f64, ytex: f64, col: i32, alpha: f64) {
        self.primitive_2d.push_vertex(
            [x as f32, y as f32, self.depth],
            [xtex as f32, ytex as f32],
            split_colour(col, alpha),
            [0.0, 0.0, 0.0],
        );
    }

    fn draw_primitive_2d(&mut self) {
        // I would use push_primitive but that causes borrowing issues.
        self.setup_queue(self.primitive_2d.get_atlas_id(), self.primitive_2d.get_shape());
        self.vertex_queue.extend_from_slice(self.primitive_2d.get_vertices());
    }

    fn get_primitive_2d(&self) -> PrimitiveBuilder {
        self.primitive_2d.clone()
    }

    fn set_primitive_2d(&mut self, prim: PrimitiveBuilder) {
        self.primitive_2d = prim;
    }

    fn reset_primitive_3d(&mut self, ptype: PrimitiveType, atlas_ref: Option<AtlasRef>) {
        if let Some(a) = atlas_ref {
            self.ensure(a);
        }
        let ar = atlas_ref.and_then(|ar| self.get_rect(ar).copied());
        self.primitive_3d = PrimitiveBuilder::new(
            ar.unwrap_or(self.white_pixel),
            ptype,
            ar.is_some(),
        );
    }

    fn vertex_3d(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        nx: f64,
        ny: f64,
        nz: f64,
        xtex: f64,
        ytex: f64,
        col: i32,
        alpha: f64,
    ) {
        self.primitive_3d.push_vertex(
            [x as f32, y as f32, z as f32],
            [xtex as f32, ytex as f32],
            split_colour(col, alpha),
            [nx as f32, ny as f32, nz as f32],
        );
    }

    fn draw_primitive_3d(&mut self) {
        // See draw_primitive_2d.
        self.setup_queue(self.primitive_3d.get_atlas_id(), self.primitive_3d.get_shape());
        self.vertex_queue.extend_from_slice(self.primitive_3d.get_vertices());
    }

    fn get_primitive_3d(&self) -> PrimitiveBuilder {
        self.primitive_3d.clone()
    }

    fn set_primitive_3d(&mut self, prim: PrimitiveBuilder) {
        self.primitive_3d = prim;
    }

    fn extend_buffers(&self, buf: &mut VertexBuffer) {
        let verts = self.primitive_3d.get_vertices();
        match self.primitive_3d.get_shape() {
            PrimitiveShape::Point => buf.points.extend_from_slice(verts),
            PrimitiveShape::Line => buf.lines.extend_from_slice(&verts[..verts.len() / 2 * 2]),
            PrimitiveShape::Triangle => buf.tris.extend_from_slice(&verts[..verts.len() / 3 * 3]),
        }
    }

    fn draw_buffers(&mut self, atlas_ref: Option<AtlasRef>, buf: &VertexBuffer) {
        if let Some(a) = atlas_ref {
            self.ensure(a);
        }
        // TODO: bench this method vs copying the buffer onto the draw queue
        self.flush_queue();
        self.update_render_state();
        self.draw_buffer(
            atlas_ref.and_then(|ar| self.get_rect(ar).copied()).unwrap_or(self.white_pixel).atlas_id,
            PrimitiveShape::Point,
            &buf.points,
        );
        self.draw_buffer(
            atlas_ref.and_then(|ar| self.get_rect(ar).copied()).unwrap_or(self.white_pixel).atlas_id,
            PrimitiveShape::Line,
            &buf.lines,
        );
        self.draw_buffer(
            atlas_ref.and_then(|ar| self.get_rect(ar).copied()).unwrap_or(self.white_pixel).atlas_id,
            PrimitiveShape::Triangle,
            &buf.tris,
        );
    }

    fn get_alpha_blending(&self) -> bool {
        self.next_render_state.alpha_blending
    }

    fn set_alpha_blending(&mut self, alphablend: bool) {
        self.next_render_state.alpha_blending = alphablend;
        self.render_state_updated = true;
    }

    fn get_colour_blending(&self) -> bool {
        self.next_render_state.texture_blend.into()
    }

    fn set_colour_blending(&mut self, modulate: bool) {
        self.next_render_state.texture_blend = modulate.into();
        self.render_state_updated = true;
    }

    fn get_blend_mode(&self) -> (BlendType, BlendType) {
        self.next_render_state.blend_mode
    }

    fn set_blend_mode(&mut self, src: BlendType, dst: BlendType) {
        self.next_render_state.blend_mode = (src, dst);
        self.render_state_updated = true;
    }

    fn get_pixel_interpolation(&self) -> bool {
        self.next_render_state.interpolate_pixels.into()
    }

    fn set_pixel_interpolation(&mut self, lerping: bool) {
        // in DX (and therefore GM) this is set per texture unit, but in GL it's per texture
        // therefore, we need to apply the setting before every draw call
        self.next_render_state.interpolate_pixels = lerping.into();
        self.render_state_updated = true;
    }

    fn get_texture_repeat(&self) -> bool {
        self.next_render_state.texture_repeat.into()
    }

    fn set_texture_repeat(&mut self, repeat: bool) {
        self.next_render_state.texture_repeat = repeat.into();
        self.render_state_updated = true;
    }

    /// Does anything that's queued to be done.
    fn flush_queue(&mut self) {
        // move the queue out of self to satisfy the borrow checker
        let mut queue = std::mem::take(&mut self.vertex_queue);
        self.draw_buffer(self.current_atlas, self.queue_type, &queue);
        // clear it and put it back so we can reuse the memory
        queue.clear();
        self.vertex_queue = queue;
    }

    fn set_view_matrix(&mut self, view: [f32; 16]) {
        self.next_render_state.view_matrix = view;
        self.render_state_updated = true;
    }

    fn set_viewproj_matrix(&mut self, view: [f32; 16], proj: [f32; 16]) {
        self.next_render_state.view_matrix = view;
        self.next_render_state.proj_matrix = proj;
        self.render_state_updated = true;
    }

    fn get_model_matrix(&self) -> [f32; 16] {
        self.next_render_state.model_matrix
    }

    fn set_model_matrix(&mut self, model: [f32; 16]) {
        self.next_render_state.model_matrix = model;
        self.render_state_updated = true;
    }

    fn mult_model_matrix(&mut self, model: [f32; 16]) {
        self.next_render_state.model_matrix = mat4mult(self.next_render_state.model_matrix, model);
        self.render_state_updated = true;
    }

    fn set_projection_ortho(&mut self, x: f64, y: f64, w: f64, h: f64, angle: f64) {
        #[rustfmt::skip]
        let proj_matrix: [f32; 16] = {
            // Squish to screen, flip vertically, and constrain z to range 1 - 32000
            [
                2.0 / w as f32, 0.0,             0.0,            0.0,
                0.0,            -2.0 / h as f32, 0.0,            0.0,
                0.0,            0.0,             1.0 / 31999.0,  0.0,
                0.0,            0.0,             -1.0 / 31999.0, 1.0,
            ]
        };

        self.set_viewproj_matrix(make_view_matrix(x, y, -16000.0, w, h, angle), proj_matrix);
    }

    fn set_projection_perspective(&mut self, x: f64, y: f64, w: f64, h: f64, angle: f64) {
        #[rustfmt::skip]
        let proj_matrix: [f32; 16] = {
            // Squish to screen, flip vertically, and constrain z to range 1 - 32000
            [
                2.0, 0.0,                  0.0,                0.0,
                0.0, 2.0 * (w / h) as f32, 0.0,                0.0,
                0.0, 0.0,                  32000.0 / 31999.0,  1.0,
                0.0, 0.0,                  -32000.0 / 31999.0, 0.0,
            ]
        };

        self.set_viewproj_matrix(make_view_matrix(x, y, -w, w, h, angle), proj_matrix);
    }

    fn set_view(
        &mut self,
        src_x: i32,
        src_y: i32,
        src_w: i32,
        src_h: i32,
        src_angle: f64,
        port_x: i32,
        port_y: i32,
        port_w: i32,
        port_h: i32,
    ) {
        self.flush_queue();
        // DX8's viewport function doesn't do anything if a surface is set as the draw target, so emulate that
        let mut fb_current = 0;
        unsafe {
            self.gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fb_current);
            assert_eq!(self.gl.GetError(), 0);
        }
        if fb_current == self.framebuffer.fbo as _ {
            // Set viewport (gl::Viewport, gl::Scissor)
            if port_x >= 0 && port_y >= 0 && port_w >= 0 && port_h >= 0 {
                unsafe {
                    self.gl.Viewport(port_x, port_y, port_w, port_h);
                    self.gl.Scissor(port_x, port_y, port_w, port_h);
                    assert_eq!(self.gl.GetError(), 0);
                }
            }
        }
        if self.using_3d && self.perspective {
            self.set_projection_perspective(src_x.into(), src_y.into(), src_w.into(), src_h.into(), src_angle);
        } else {
            self.set_projection_ortho(src_x.into(), src_y.into(), src_w.into(), src_h.into(), src_angle);
        }
    }

    fn clear_view(&mut self, colour: Colour, alpha: f64) {
        self.flush_queue();
        unsafe {
            self.gl.ClearColor(colour.r as f32, colour.g as f32, colour.b as f32, alpha as f32);
            self.gl.Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn clear_view_no_zbuf(&mut self, colour: Colour, alpha: f64) {
        self.flush_queue();
        unsafe {
            self.gl.ClearColor(colour.r as f32, colour.g as f32, colour.b as f32, alpha as f32);
            self.gl.Clear(gl::COLOR_BUFFER_BIT);
            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn clear_zbuf(&mut self) {
        if self.using_3d {
            self.flush_queue();
            unsafe {
                self.gl.Clear(gl::DEPTH_BUFFER_BIT);
            }
        }
    }

    fn get_3d(&self) -> bool {
        self.using_3d
    }

    fn set_3d(&mut self, use_3d: bool) {
        self.using_3d = use_3d;
        self.set_depth_test(use_3d);
        self.set_perspective(use_3d);
    }

    fn get_depth(&self) -> f32 {
        self.depth
    }

    fn set_depth(&mut self, depth: f32) {
        self.depth = if self.using_3d { depth.max(-16000.0).min(16000.0) } else { 0.0 };
    }

    fn get_depth_test(&self) -> bool {
        self.next_render_state.depth_test.into()
    }

    fn set_depth_test(&mut self, depth_test: bool) {
        let depth_test = depth_test && self.using_3d;
        self.next_render_state.depth_test = depth_test.into();
        self.render_state_updated = true;
    }

    fn get_write_depth(&self) -> bool {
        self.next_render_state.write_depth
    }

    fn set_write_depth(&mut self, write_depth: bool) {
        self.next_render_state.write_depth = write_depth;
        self.render_state_updated = true;
    }

    fn get_culling(&self) -> bool {
        self.next_render_state.culling
    }

    fn set_culling(&mut self, culling: bool) {
        self.next_render_state.culling = culling;
        self.render_state_updated = true;
    }

    fn get_perspective(&self) -> bool {
        self.perspective
    }

    fn set_perspective(&mut self, perspective: bool) {
        // don't need to flush_queue for this because this only affects set_view
        self.perspective = perspective;
    }

    fn get_fog(&self) -> Option<Fog> {
        let c = self.next_render_state.fog_colour;
        bool::from(self.next_render_state.fog_enabled).then(|| Fog {
            colour: u32::from(Colour::from((f64::from(c[0]), c[1].into(), c[2].into()))) as i32,
            begin: self.next_render_state.fog_begin,
            end: self.next_render_state.fog_end,
        })
    }

    fn set_fog(&mut self, fog: Option<Fog>) {
        self.next_render_state.fog_enabled = fog.is_some().into();
        if let Some(fog) = fog {
            self.next_render_state.fog_colour = split_colour(fog.colour, 1.0);
            self.next_render_state.fog_begin = fog.begin;
            self.next_render_state.fog_end = fog.end;
        }
        self.render_state_updated = true;
    }

    fn get_gouraud(&self) -> bool {
        self.next_render_state.gouraud.into()
    }

    fn set_gouraud(&mut self, gouraud: bool) {
        self.next_render_state.gouraud = gouraud.into();
        self.render_state_updated = true;
    }

    fn get_lighting_enabled(&self) -> bool {
        self.next_render_state.lighting.into()
    }

    fn set_lighting_enabled(&mut self, enabled: bool) {
        self.next_render_state.lighting = enabled.into();
        self.render_state_updated = true;
    }

    fn get_ambient_colour(&self) -> i32 {
        let col = &self.next_render_state.ambient_colour[0..3];
        u32::from(Colour::from((f64::from(col[0]), col[1].into(), col[2].into()))) as _
    }

    fn set_ambient_colour(&mut self, colour: i32) {
        self.next_render_state.ambient_colour = split_colour(colour, 1.0);
        self.render_state_updated = true;
    }

    fn get_lights(&self) -> [(bool, Light); 8] {
        let mut iter = self.next_render_state.lights.iter().map(|l| {
            let position = [l.pos[0], l.pos[1], l.pos[2]];
            let colour =
                u32::from(Colour::from((f64::from(l.colour[0]), l.colour[1].into(), l.colour[2].into()))) as i32;
            (
                l.enabled.into(),
                if l.is_point.into() {
                    Light::Point { position, range: l.range, colour }
                } else {
                    Light::Directional { direction: position, colour }
                },
            )
        });
        [
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        ]
    }

    fn set_lights(&mut self, lights: [(bool, Light); 8]) {
        lights.iter().enumerate().for_each(|(i, &(enabled, light))| {
            self.set_light_enabled(i, enabled);
            self.set_light(i, light);
        })
    }

    fn set_light_enabled(&mut self, id: usize, enabled: bool) {
        self.next_render_state.lights[id].enabled = enabled.into();
        self.render_state_updated = true;
    }

    fn set_light(&mut self, id: usize, light: Light) {
        match light {
            Light::Directional { direction, colour } => {
                self.next_render_state.lights[id].is_point = false.into();
                self.next_render_state.lights[id].pos[0..3].copy_from_slice(&direction);
                self.next_render_state.lights[id].colour = split_colour(colour, 1.0);
            },
            Light::Point { position, range, colour } => {
                self.next_render_state.lights[id].is_point = true.into();
                self.next_render_state.lights[id].pos[0..3].copy_from_slice(&position);
                self.next_render_state.lights[id].colour = split_colour(colour, 1.0);
                self.next_render_state.lights[id].range = range;
            },
        }
        self.render_state_updated = true;
    }

    fn present(&mut self, window_width: u32, window_height: u32, scaling: Scaling) {
        if window_width == 0 || window_height == 0 {
            // if we continue, intel will dereference a null pointer
            return
        }
        unsafe {
            // Finish drawing frame
            self.flush_queue();

            // Get framebuffer size
            let (mut fb_width, mut fb_height) = (0, 0);
            self.gl.BindTexture(gl::TEXTURE_2D, self.framebuffer.texture);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut fb_width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut fb_height);

            // yeah i know but they need to be converted anyway
            let (window_width, window_height) = (window_width as i32, window_height as i32);

            // Scaling
            let (w_x, w_y, w_w, w_h) = match scaling {
                Scaling::Fixed(scale) => {
                    let w = (f64::from(fb_width) * scale) as i32;
                    let h = (f64::from(fb_height) * scale) as i32;
                    ((window_width - w) / 2, (window_height - h) / 2, w, h)
                },
                Scaling::Aspect(_) => {
                    if fb_width > 0 && fb_height > 0 {
                        let fixed_width = window_height * fb_width / fb_height;
                        if fixed_width < window_width {
                            // window is too wide
                            ((window_width - fixed_width) / 2, 0, fixed_width, window_height)
                        } else {
                            // window is too tall
                            let fixed_height = window_width * fb_height / fb_width;
                            (0, (window_height - fixed_height) / 2, window_width, fixed_height)
                        }
                    } else {
                        // can never be too careful
                        (0, 0, fb_width, fb_height)
                    }
                },
                Scaling::Full => (0, 0, window_width, window_height),
            };

            // PS Vita: escalado entero centrado (320×180 → 960×540) dibujando un rectángulo con la
            // textura del búfer; vitaGL no tiene glBlitFramebuffer entre búferes de lectura y dibujo.
            if self.simple.is_some() && (cfg!(target_os = "vita") || std::env::var_os("RLD_QUAD_PRESENT").is_some()) {
                self.present_quad(window_width, window_height, fb_width, fb_height);
                self.imp.swap_buffers();
                self.lazy.frame += 1;
                self.gl.GetError();
                return
            }

            // On Intel, glBlitFrameBuffer just does nothing if the scissor box is too big, which it
            // very well could be. So just disable the scissor test for now.
            self.gl.Disable(gl::SCISSOR_TEST);

            // Remember old framebuffer so we can rebind it after we're done
            let mut fb_old = 0;
            self.gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fb_old);
            assert_eq!(self.gl.GetError(), 0);

            // Draw framebuffer to screen
            self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            assert_eq!(self.gl.GetError(), 0);
            self.clear_view((0.0, 0.0, 0.0).into(), 1.0); // to avoid weird strobe lights (???)
            assert_eq!(self.gl.GetError(), 0);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, self.framebuffer.fbo);
            assert_eq!(self.gl.GetError(), 0);
            self.gl.BlitFramebuffer(
                0,
                fb_height,
                fb_width,
                0,
                w_x,
                w_y,
                w_x + w_w,
                w_y + w_h,
                gl::COLOR_BUFFER_BIT,
                if self.next_render_state.interpolate_pixels.into() { gl::LINEAR } else { gl::NEAREST },
            );
            assert_eq!(self.gl.GetError(), 0);
            self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, fb_old as u32);
            assert_eq!(self.gl.GetError(), 0);

            self.gl.Enable(gl::SCISSOR_TEST);

            assert_eq!(self.gl.GetError(), 0);

            // Present buffer
            // Note: Game Maker always presents the backbuffer, even when the target is a surface.
            self.imp.swap_buffers();
            self.lazy.frame += 1;

            // On Nvidia/AMD cards, unless the emulator is running in admin mode, if it is screenshared on Discord,
            // SwapBuffers will return TRUE i.e. no error, but glGetError will return GL_INVALID_OPERATION.
            // This hack evades the error, but a less awful solution would be really nice to have.
            self.gl.GetError();
        }
    }

    fn draw_stored(&mut self, x: i32, y: i32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return
        }

        let framebuffer = match self.stored_framebuffer {
            Some(f) => f,
            None => return,
        };

        self.flush_queue();

        unsafe {
            // Get framebuffer size
            let (mut fb_width, mut fb_height) = (0, 0);
            self.gl.BindTexture(gl::TEXTURE_2D, framebuffer.texture);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut fb_width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut fb_height);

            let (width, height) = (w as i32, h as i32);

            // Remember old framebuffer so we can rebind it after we're done
            let mut fb_old = 0;
            self.gl.GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fb_old);
            assert_eq!(self.gl.GetError(), 0);

            // Draw framebuffer to screen
            self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.framebuffer.fbo);
            self.gl.BindFramebuffer(gl::READ_FRAMEBUFFER, framebuffer.fbo);
            self.gl.BlitFramebuffer(
                0,
                0,
                fb_width,
                fb_height,
                x,
                y,
                x + width,
                y + height,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            self.gl.BindFramebuffer(gl::DRAW_FRAMEBUFFER, fb_old as u32);

            assert_eq!(self.gl.GetError(), 0);
        }
    }

    fn stored_size(&self) -> (u32, u32) {
        let framebuffer = self.stored_framebuffer.unwrap_or(self.framebuffer);
        unsafe {
            let (mut width, mut height) = (0, 0);
            self.gl.BindTexture(gl::TEXTURE_2D, framebuffer.texture);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_WIDTH, &mut width);
            self.gl.GetTexLevelParameteriv(gl::TEXTURE_2D, 0, gl::TEXTURE_HEIGHT, &mut height);
            (
                u32::try_from(width).expect("Negative width reported by OpenGL"),
                u32::try_from(height).expect("Negative height reported by OpenGL"),
            )
        }
    }

    fn finish(&mut self, window_width: u32, window_height: u32, clear_colour: Colour) {
        // Present screen
        self.present(window_width, window_height, Scaling::Fixed(1.0));

        // Start next frame
        self.setup_frame(clear_colour)
    }
}
