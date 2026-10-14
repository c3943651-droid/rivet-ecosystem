//! Metrics (RIVET.md §12): EGL context init, shader compilation, and
//! frame time (render + 1080p readback, v1 wl_shm path) with
//! 1k/10k/100k rectangles. Requires EGL (Mesa); skipped if absent.
//!
//! `cargo bench -p rivet-canvas`

use std::hint::black_box;
use std::time::Instant;

use criterion::Criterion;
use rivet_canvas::glow::HasContext as _;
use rivet_canvas::{Color, GraphicsContext, Rect, RenderQueue};

const W: u32 = 1920;
const H: u32 = 1080;

#[allow(dead_code)]
/// Contexto EGL headless completo (igual que rivet-ui::app).
struct Egl {
    egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_4>,
    display: khronos_egl::Display,
    config: khronos_egl::Config,
    context: khronos_egl::Context,
    surface: khronos_egl::Surface,
}

fn egl_setup(
    egl: &khronos_egl::DynamicInstance<khronos_egl::EGL1_4>,
    display: khronos_egl::Display,
) -> Result<
    (
        khronos_egl::Config,
        khronos_egl::Context,
        khronos_egl::Surface,
    ),
    String,
> {
    let config_attribs: &[khronos_egl::Int] = &[
        khronos_egl::RED_SIZE,
        8,
        khronos_egl::GREEN_SIZE,
        8,
        khronos_egl::BLUE_SIZE,
        8,
        khronos_egl::ALPHA_SIZE,
        8,
        khronos_egl::SURFACE_TYPE,
        khronos_egl::PBUFFER_BIT,
        khronos_egl::NONE,
    ];
    let mut configs = Vec::with_capacity(8);
    egl.choose_config(display, config_attribs, &mut configs)
        .map_err(|e| format!("eglChooseConfig: {e:?}"))?;
    let config = *configs.first().ok_or("sin configs EGL")?;
    egl.bind_api(khronos_egl::OPENGL_API)
        .map_err(|e| format!("{e:?}"))?;
    let context_attribs: &[khronos_egl::Int] = &[
        khronos_egl::CONTEXT_MAJOR_VERSION,
        3,
        khronos_egl::CONTEXT_MINOR_VERSION,
        3,
        khronos_egl::NONE,
    ];
    let context = egl
        .create_context(display, config, None, context_attribs)
        .map_err(|e| format!("eglCreateContext: {e:?}"))?;
    let surface_attribs: &[khronos_egl::Int] = &[
        khronos_egl::WIDTH,
        64,
        khronos_egl::HEIGHT,
        64,
        khronos_egl::NONE,
    ];
    let surface = egl
        .create_pbuffer_surface(display, config, surface_attribs)
        .map_err(|e| format!("eglCreatePbufferSurface: {e:?}"))?;
    egl.make_current(display, Some(surface), Some(surface), Some(context))
        .map_err(|e| format!("eglMakeCurrent: {e:?}"))?;
    Ok((config, context, surface))
}

fn contexto_completo() -> Result<Egl, String> {
    // SAFETY: carga dinámica estándar de la libEGL del sistema.
    let egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_4> =
        unsafe { khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required() }
            .map_err(|e| e.to_string())?;
    let display =
        unsafe { egl.get_display(khronos_egl::DEFAULT_DISPLAY) }.ok_or("eglGetDisplay falló")?;
    egl.initialize(display).map_err(|e| format!("{e:?}"))?;
    let (config, context, surface) = egl_setup(&egl, display)?;
    Ok(Egl {
        egl,
        display,
        config,
        context,
        surface,
    })
}

/// §14: "eglInitialize + contexto" (< 50 ms objetivo).
fn bench_egl_init(c: &mut Criterion) {
    c.bench_function("canvas/egl init + contexto 3.3", |b| {
        b.iter_custom(|iters| {
            let mut total = std::time::Duration::ZERO;
            for _ in 0..iters {
                let egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_4> =
                    unsafe { khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required() }
                        .expect("libEGL");
                let t = Instant::now();
                let display =
                    unsafe { egl.get_display(khronos_egl::DEFAULT_DISPLAY) }.expect("display");
                egl.initialize(display).expect("initialize");
                let (_config, context, surface) = egl_setup(&egl, display).expect("contexto");
                total += t.elapsed();
                egl.make_current(display, None, None, None)
                    .expect("release");
                egl.destroy_context(display, context).expect("destroy");
                egl.destroy_surface(display, surface).expect("destroy");
                egl.terminate(display).expect("terminate");
            }
            total
        })
    });
}

/// §14: "compilación de shaders" (< 30 ms objetivo). Medido como
/// `GraphicsContext::from_loader` sobre el contexto ya inicializado
/// (loader + 2 programas: flat + textura).
fn bench_shaders(c: &mut Criterion, egl: &Egl) {
    c.bench_function("canvas/compilación de shaders", |b| {
        b.iter(|| {
            let gl = GraphicsContext::from_loader(
                |name| {
                    egl.egl
                        .get_proc_address(name)
                        .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
                },
                64,
                64,
            );
            black_box(gl.is_ok())
        })
    });
}

/// §14: frame time con 1k/10k/100k rects (render + readback 1080p —
/// la ruta real v1 wl_shm; sin VSync: FBO headless).
fn bench_frames(c: &mut Criterion, gl: &mut GraphicsContext) {
    let (fbo, tex) = gl.create_render_target(W, H).expect("FBO 1080p");
    gl.resize(W, H);
    for &n in &[1_000usize, 10_000, 100_000] {
        let mut queue = RenderQueue::new();
        for i in 0..n {
            let x = ((i * 7) % (W as usize)) as f32;
            let y = ((i * 13) % (H as usize)) as f32;
            queue.rect(
                0,
                Rect::new(x, y, 32.0, 32.0),
                Color::opaque(
                    ((i * 31) % 255) as f32 / 255.0,
                    ((i * 57) % 255) as f32 / 255.0,
                    ((i * 97) % 255) as f32 / 255.0,
                ),
            );
        }
        let label = format!("canvas/frame {n} rects (render+readback 1080p)");
        c.bench_function(&label, |b| {
            b.iter(|| {
                gl.set_target(Some(fbo));
                let stats = gl.render(black_box(&queue));
                let pixels = gl.read_pixels(0, 0, W, H);
                black_box((stats, pixels.len()))
            })
        });
    }
    gl.destroy_render_target(fbo, tex);
}

fn main() {
    let mut c = Criterion::default().configure_from_args();
    let egl = match contexto_completo() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("SKIP benches GL: sin EGL ({e})");
            return;
        }
    };
    eprintln!(
        "GL VENDOR/RENDERER: {} / {}",
        gl_string(&egl, rivet_canvas::glow::VENDOR),
        gl_string(&egl, rivet_canvas::glow::RENDERER)
    );
    let mut gl = GraphicsContext::from_loader(
        |name| {
            egl.egl
                .get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
        },
        64,
        64,
    )
    .expect("GraphicsContext");
    bench_egl_init(&mut c);
    bench_shaders(&mut c, &egl);
    bench_frames(&mut c, &mut gl);
}

/// glGetString vía glow sobre el contexto ya inicializado.
fn gl_string(egl: &Egl, name: u32) -> String {
    let gl = unsafe {
        rivet_canvas::glow::Context::from_loader_function(|n| {
            egl.egl
                .get_proc_address(n)
                .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
        })
    };
    unsafe { gl.get_parameter_string(name) }
}
