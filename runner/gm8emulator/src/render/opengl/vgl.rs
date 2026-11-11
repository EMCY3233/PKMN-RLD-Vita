//! Plataforma OpenGL para PS Vita: vitaGL (OpenGL sobre sceGxm, la GPU PowerVR SGX543MP4+).
//!
//! El renderizador de OpenGMK usa algunas funciones que vitaGL no tiene. En vez de cambiar el
//! renderizador, el cargador de funciones las sustituye por versiones equivalentes:
//! - `glGetTexLevelParameteriv` (ancho/alto de textura): se recuerdan los tamaños al crear texturas.
//! - Texturas y adjuntos de profundidad: el juego es 2D y no usa búfer Z; se ignoran.
//! - `glProvokingVertex`: solo afecta al sombreado plano en 3D; sin efecto.
//! - `glGetTexImage`: solo para estados guardados del emulador; devuelve ceros.
//! - `glGetError`: vitaGL marca errores en llamadas inocuas; se apuntan en runner.log una vez
//!   y no detienen el juego.

use ramen::{connection::Connection, window::Window};
use std::{
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    os::raw::{c_char, c_int, c_uint},
    sync::Mutex,
};

type GLenum = c_uint;
type GLuint = c_uint;
type GLint = c_int;
type GLsizei = c_int;

const GL_TEXTURE_2D: GLenum = 0x0DE1;
const GL_TEXTURE_WIDTH: GLenum = 0x1000;
const GL_TEXTURE_HEIGHT: GLenum = 0x1001;
const GL_DEPTH_COMPONENT: GLenum = 0x1902;
const GL_DEPTH_ATTACHMENT: GLenum = 0x8D00;
const GL_BGRA: GLenum = 0x80E1;
const GL_FRAMEBUFFER: GLenum = 0x8D40;
const GL_READ_FRAMEBUFFER: GLenum = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
const GL_DRAW_FRAMEBUFFER_BINDING: GLenum = 0x8CA6;
const GL_READ_FRAMEBUFFER_BINDING: GLenum = 0x8CAA;

/// vitaGL solo conoce GL_FRAMEBUFFER (enlazar GL_READ_FRAMEBUFFER da GL_INVALID_ENUM y adjuntar
/// una textura a él cerraba el juego). Se emulan dos enlaces separados: el de dibujo es el real y
/// el de lectura se enlaza solo durante las operaciones que leen (copiar, leer píxeles, adjuntar).
static FB: Mutex<(GLuint, GLuint)> = Mutex::new((0, 0)); // (dibujo, lectura)

unsafe fn with_read_fb<R>(f: impl FnOnce() -> R) -> R {
    let (draw, read) = *FB.lock().unwrap();
    if read != draw {
        real(|r| (r.bind_framebuffer)(GL_FRAMEBUFFER, read));
    }
    let res = f();
    if read != draw {
        real(|r| (r.bind_framebuffer)(GL_FRAMEBUFFER, draw));
    }
    res
}

unsafe extern "C" fn shim_bind_framebuffer(target: GLenum, fb: GLuint) {
    let mut st = FB.lock().unwrap();
    match target {
        GL_READ_FRAMEBUFFER => st.1 = fb,
        GL_DRAW_FRAMEBUFFER => {
            st.0 = fb;
            drop(st);
            real(|r| (r.bind_framebuffer)(GL_FRAMEBUFFER, fb));
        },
        _ => {
            st.0 = fb;
            st.1 = fb;
            drop(st);
            real(|r| (r.bind_framebuffer)(GL_FRAMEBUFFER, fb));
        },
    }
}

unsafe extern "C" fn shim_get_integerv(pname: GLenum, out: *mut GLint) {
    match pname {
        GL_DRAW_FRAMEBUFFER_BINDING => *out = FB.lock().unwrap().0 as GLint,
        GL_READ_FRAMEBUFFER_BINDING => *out = FB.lock().unwrap().1 as GLint,
        _ => real(|r| (r.get_integerv)(pname, out)),
    }
}

unsafe extern "C" fn shim_read_pixels(x: GLint, y: GLint, w: GLsizei, h: GLsizei, f: GLenum, t: GLenum, out: *mut c_void) {
    with_read_fb(|| real(|r| (r.read_pixels)(x, y, w, h, f, t, out)))
}
const GL_RGBA: GLenum = 0x1908;
const GL_UNSIGNED_BYTE: GLenum = 0x1401;

/// vitaGL no acepta píxeles BGRA (el formato de GM8): se convierten a RGBA al subirlos
unsafe fn bgra_to_rgba(format: GLenum, ty: GLenum, w: GLsizei, h: GLsizei, data: *const c_void) -> (GLenum, Option<Vec<u8>>) {
    if format != GL_BGRA || ty != GL_UNSIGNED_BYTE {
        return (format, None)
    }
    if data.is_null() || w <= 0 || h <= 0 {
        return (GL_RGBA, None)
    }
    let mut v = std::slice::from_raw_parts(data as *const u8, (w * h * 4) as usize).to_vec();
    for px in v.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    (GL_RGBA, Some(v))
}

extern "C" {
    fn vglMemFree(t: c_int) -> usize;
    fn vglInitExtended(legacy_pool_size: c_int, width: c_int, height: c_int, ram_threshold: c_int, msaa: c_int) -> u8;
    fn vglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn vglSetSemanticBindingMode(mode: c_int);
    fn vglSwapBuffers(has_commondialog: u8);
    fn vglWaitVblankStart(enable: u8);
}

/// Funciones reales de vitaGL que los sustitutos necesitan
struct Real {
    bind_texture: unsafe extern "C" fn(GLenum, GLuint),
    tex_image_2d: unsafe extern "C" fn(GLenum, GLint, GLint, GLsizei, GLsizei, GLint, GLenum, GLenum, *const c_void),
    tex_sub_image_2d: unsafe extern "C" fn(GLenum, GLint, GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *const c_void),
    copy_tex_image_2d: unsafe extern "C" fn(GLenum, GLint, GLenum, GLint, GLint, GLsizei, GLsizei, GLint),
    delete_textures: unsafe extern "C" fn(GLsizei, *const GLuint),
    framebuffer_texture_2d: unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint, GLint),
    get_error: unsafe extern "C" fn() -> GLenum,
    bind_framebuffer: unsafe extern "C" fn(GLenum, GLuint),
    get_integerv: unsafe extern "C" fn(GLenum, *mut GLint),
    read_pixels: unsafe extern "C" fn(GLint, GLint, GLsizei, GLsizei, GLenum, GLenum, *mut c_void),
}

static REAL: Mutex<Option<Real>> = Mutex::new(None);
/// textura enlazada (el renderizador solo usa la unidad 0) y tamaños conocidos
static TEX: Mutex<(GLuint, Option<HashMap<GLuint, (GLint, GLint)>>)> = Mutex::new((0, None));
static ERRORS_LOGGED: Mutex<u32> = Mutex::new(0);

fn real<R>(f: impl FnOnce(&Real) -> R) -> R {
    f(REAL.lock().unwrap().as_ref().expect("vitaGL no inicializado"))
}

fn remember_size(w: GLint, h: GLint) {
    let mut t = TEX.lock().unwrap();
    let bound = t.0;
    t.1.get_or_insert_with(HashMap::new).insert(bound, (w, h));
}

unsafe extern "C" fn shim_bind_texture(target: GLenum, tex: GLuint) {
    if target == GL_TEXTURE_2D {
        TEX.lock().unwrap().0 = tex;
    }
    real(|r| (r.bind_texture)(target, tex))
}

unsafe extern "C" fn shim_tex_image_2d(
    target: GLenum,
    level: GLint,
    internal: GLint,
    w: GLsizei,
    h: GLsizei,
    border: GLint,
    format: GLenum,
    ty: GLenum,
    data: *const c_void,
) {
    if target == GL_TEXTURE_2D && level == 0 {
        remember_size(w, h);
    }
    if format == GL_DEPTH_COMPONENT {
        return // sin búfer Z en la Vita (juego 2D)
    }
    let (format, conv) = bgra_to_rgba(format, ty, w, h, data);
    let data = conv.as_ref().map_or(data, |v| v.as_ptr().cast());
    let internal = if internal == GL_BGRA as GLint { GL_RGBA as GLint } else { internal };
    real(|r| (r.tex_image_2d)(target, level, internal, w, h, border, format, ty, data))
}

unsafe extern "C" fn shim_tex_sub_image_2d(
    target: GLenum,
    level: GLint,
    x: GLint,
    y: GLint,
    w: GLsizei,
    h: GLsizei,
    format: GLenum,
    ty: GLenum,
    data: *const c_void,
) {
    let (format, conv) = bgra_to_rgba(format, ty, w, h, data);
    let data = conv.as_ref().map_or(data, |v| v.as_ptr().cast());
    real(|r| (r.tex_sub_image_2d)(target, level, x, y, w, h, format, ty, data))
}

unsafe extern "C" fn shim_copy_tex_image_2d(
    target: GLenum,
    level: GLint,
    internal: GLenum,
    x: GLint,
    y: GLint,
    w: GLsizei,
    h: GLsizei,
    border: GLint,
) {
    if target == GL_TEXTURE_2D && level == 0 {
        remember_size(w, h);
    }
    with_read_fb(|| real(|r| (r.copy_tex_image_2d)(target, level, internal, x, y, w, h, border)))
}

unsafe extern "C" fn shim_delete_textures(n: GLsizei, ids: *const GLuint) {
    if !ids.is_null() && n > 0 {
        let mut t = TEX.lock().unwrap();
        if let Some(m) = t.1.as_mut() {
            for &id in std::slice::from_raw_parts(ids, n as usize) {
                m.remove(&id);
            }
        }
    }
    real(|r| (r.delete_textures)(n, ids))
}

unsafe extern "C" fn shim_get_tex_level_parameteriv(_target: GLenum, _level: GLint, pname: GLenum, out: *mut GLint) {
    let t = TEX.lock().unwrap();
    let (w, h) = t.1.as_ref().and_then(|m| m.get(&t.0).copied()).unwrap_or((0, 0));
    if !out.is_null() {
        *out = match pname {
            GL_TEXTURE_WIDTH => w,
            GL_TEXTURE_HEIGHT => h,
            _ => 0,
        };
    }
}

unsafe extern "C" fn shim_framebuffer_texture_2d(target: GLenum, attach: GLenum, textarget: GLenum, tex: GLuint, level: GLint) {
    if attach == GL_DEPTH_ATTACHMENT {
        return
    }
    if target == GL_READ_FRAMEBUFFER {
        with_read_fb(|| real(|r| (r.framebuffer_texture_2d)(GL_FRAMEBUFFER, attach, textarget, tex, level)))
    } else {
        real(|r| (r.framebuffer_texture_2d)(GL_FRAMEBUFFER, attach, textarget, tex, level))
    }
}

unsafe extern "C" fn shim_provoking_vertex(_mode: GLenum) {}

/// glBlitFramebuffer necesita enlaces de lectura y dibujo separados, que vitaGL no tiene. El
/// renderizador ya no lo usa para presentar en Vita (dibuja un rectángulo con la textura); queda
/// el redimensionado del búfer al cambiar de tamaño de sala, que se redibuja en el fotograma
/// siguiente de todas formas.
unsafe extern "C" fn shim_blit_framebuffer(
    _a: GLint, _b: GLint, _c: GLint, _d: GLint, _e: GLint, _f: GLint, _g: GLint, _h: GLint, _mask: GLenum, _filter: GLenum,
) {
}

unsafe extern "C" fn shim_get_tex_image(_target: GLenum, _level: GLint, _format: GLenum, _ty: GLenum, _out: *mut c_void) {
    // los volcados de texturas solo sirven para estados guardados del emulador, que el port no usa
}

unsafe extern "C" fn shim_get_error() -> GLenum {
    let e = real(|r| (r.get_error)());
    if e != 0 {
        let mut n = ERRORS_LOGGED.lock().unwrap();
        *n += 1;
        if *n <= 20 {
            crate::startlog(&format!("vitaGL: error GL 0x{:04X} (ignorado)", e));
        }
    }
    0
}

/// Memoria libre en los montones de vitaGL (para el registro de rendimiento)
pub fn mem_report() -> String {
    let mb = |t| unsafe { vglMemFree(t) } / 1048576;
    format!(" | vitaGL libre: CDRAM {} MB, RAM {} MB, phycont {} MB", mb(0), mb(1), mb(2))
}

/// (RAM de usuario, CDRAM, phycont) libres, en bytes
fn free_memory() -> (i32, i32, i32) {
    let mut info = vitasdk_sys::SceKernelFreeMemorySizeInfo {
        size: std::mem::size_of::<vitasdk_sys::SceKernelFreeMemorySizeInfo>() as i32,
        size_user: 0,
        size_cdram: 0,
        size_phycont: 0,
    };
    unsafe {
        vitasdk_sys::sceKernelGetFreeMemorySize(&mut info);
    }
    (info.size_user, info.size_cdram, info.size_phycont)
}

fn shader_compiler_present() -> bool {
    std::fs::metadata("ur0:data/libshacccg.suprx").is_ok() || std::fs::metadata("ur0:/data/libshacccg.suprx").is_ok()
}

pub struct PlatformImpl {
    vsync: std::cell::Cell<u32>,
}

impl PlatformImpl {
    pub unsafe fn new(_connection: &Connection, _window: &Window) -> Result<Self, String> {
        // Los shaders se compilan en la consola con libshacccg.suprx (ur0:data/libshacccg.suprx)
        let free = free_memory();
        crate::startlog(&format!(
            "Memoria libre antes de vitaGL: RAM {} MB, CDRAM {} MB, phycont {} MB",
            free.0 / 1048576,
            free.1 / 1048576,
            free.2 / 1048576
        ));
        if !shader_compiler_present() {
            crate::startlog("AVISO: no existe ur0:data/libshacccg.suprx (vitaGL no podrá compilar shaders)");
        }
        // Ojo: vglInit* devuelve GL_TRUE solo si tuvo que bajar la resolución y GL_FALSE en un inicio
        // normal (ver vgl.c: `return res_fallback;`). No es un indicador de error: si falla, vitaGL
        // no vuelve. Un segundo intento tras un inicio correcto se rechaza ("already inited").
        let mb = 1024 * 1024;
        let fallback = vglInitExtended(0, 960, 544, 16 * mb, 0) != 0;
        // Traducir y compilar cada shader en glCompileShader (modo «par de shaders») en vez de
        // aplazarlo a glLinkProgram: así un error de compilación se detecta y se apunta en
        // runner.log, en vez de enlazar un programa vacío (cierre en sceGxm).
        vglSetSemanticBindingMode(0 /* VGL_MODE_SHADER_PAIR */);
        let after = free_memory();
        crate::startlog(&format!(
            "vitaGL iniciado{} (libre después: RAM {} MB, CDRAM {} MB, phycont {} MB)",
            if fallback { " con resolución reducida" } else { "" },
            after.0 / mb,
            after.1 / mb,
            after.2 / mb
        ));
        let load = |n: &str| {
            let c = CString::new(n).unwrap();
            let p = vglGetProcAddress(c.as_ptr());
            if p.is_null() {
                Err(format!("vitaGL no tiene {}", n))
            } else {
                Ok(p)
            }
        };
        *REAL.lock().unwrap() = Some(Real {
            bind_texture: std::mem::transmute(load("glBindTexture")?),
            tex_image_2d: std::mem::transmute(load("glTexImage2D")?),
            tex_sub_image_2d: std::mem::transmute(load("glTexSubImage2D")?),
            copy_tex_image_2d: std::mem::transmute(load("glCopyTexImage2D")?),
            delete_textures: std::mem::transmute(load("glDeleteTextures")?),
            framebuffer_texture_2d: std::mem::transmute(load("glFramebufferTexture2D")?),
            get_error: std::mem::transmute(load("glGetError")?),
            bind_framebuffer: std::mem::transmute(load("glBindFramebuffer")?),
            get_integerv: std::mem::transmute(load("glGetIntegerv")?),
            read_pixels: std::mem::transmute(load("glReadPixels")?),
        });
        Ok(PlatformImpl { vsync: std::cell::Cell::new(1) })
    }

    pub unsafe fn get_function_loader() -> Result<Box<dyn FnMut(&'static str) -> *const c_void>, String> {
        Ok(Box::new(|name: &'static str| unsafe {
            let shim: *const c_void = match name {
                "glBindTexture" => shim_bind_texture as *const c_void,
                "glTexImage2D" => shim_tex_image_2d as *const c_void,
                "glTexSubImage2D" => shim_tex_sub_image_2d as *const c_void,
                "glCopyTexImage2D" => shim_copy_tex_image_2d as *const c_void,
                "glDeleteTextures" => shim_delete_textures as *const c_void,
                "glGetTexLevelParameteriv" => shim_get_tex_level_parameteriv as *const c_void,
                "glFramebufferTexture2D" => shim_framebuffer_texture_2d as *const c_void,
                "glProvokingVertex" => shim_provoking_vertex as *const c_void,
                "glGetTexImage" => shim_get_tex_image as *const c_void,
                "glGetError" => shim_get_error as *const c_void,
                "glBindFramebuffer" => shim_bind_framebuffer as *const c_void,
                "glGetIntegerv" => shim_get_integerv as *const c_void,
                "glReadPixels" => shim_read_pixels as *const c_void,
                "glBlitFramebuffer" => shim_blit_framebuffer as *const c_void,
                // ReadBuffer(GL_BACK) no aplica en vitaGL
                "glReadBuffer" => shim_provoking_vertex as *const c_void,
                _ => std::ptr::null(),
            };
            if !shim.is_null() {
                return shim
            }
            let c = CString::new(name).unwrap();
            vglGetProcAddress(c.as_ptr()) as *const c_void
        }))
    }

    pub unsafe fn clean_function_loader() {}

    pub unsafe fn swap_buffers(&self) {
        // con el teclado del sistema en pantalla, vitaGL debe dejar dibujar el diálogo
        vglSwapBuffers(crate::game::vita_input::IME_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) as u8);
    }

    pub unsafe fn set_swap_interval(&self, n: u32) -> bool {
        self.vsync.set(n);
        vglWaitVblankStart((n != 0) as u8);
        true
    }

    pub unsafe fn get_swap_interval(&self) -> u32 {
        self.vsync.get()
    }

    pub unsafe fn wait_vsync(&self) {}
}

/// Texto de una cadena GL (para el registro)
#[allow(dead_code)]
pub unsafe fn gl_str(p: *const u8) -> String {
    if p.is_null() { String::new() } else { CStr::from_ptr(p.cast()).to_string_lossy().into_owned() }
}
