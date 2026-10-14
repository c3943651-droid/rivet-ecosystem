//! GPU integration tests with a headless EGL context.
//!
//! Si no hay `libEGL`/driver GL disponibles (p. ej. CI sin GPU), los tests se
//! saltan con un aviso en lugar de fallar. En CI se usa Mesa llvmpipe
//! (`LIBGL_ALWAYS_SOFTWARE=1`).

use khronos_egl::{self as egl, Surface};
use rivet_canvas::{Color, GraphicsContext, Rect, RenderQueue};

/// Contexto GL headless de un solo uso (EGL queda vivo por leak deliberado:
/// el GraphicsContext usa sus punteros de función durante todo el test).
struct Headless {
    #[allow(dead_code)]
    egl: egl::DynamicInstance<egl::EGL1_4>,
    #[allow(dead_code)]
    context: egl::Context,
    #[allow(dead_code)]
    surface: Surface,
}

fn make_context() -> Option<GraphicsContext> {
    let egl = match unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() } {
        Ok(egl) => egl,
        Err(e) => {
            eprintln!("skip: sin libEGL ({e})");
            return None;
        }
    };
    let display = match unsafe { egl.get_display(egl::DEFAULT_DISPLAY) } {
        Some(d) => d,
        None => {
            eprintln!("skip: eglGetDisplay falló");
            return None;
        }
    };
    if let Err(e) = egl.initialize(display) {
        eprintln!("skip: eglInitialize falló ({e:?})");
        return None;
    }
    // Config para pbuffer RGBA8888.
    let config_attribs: &[egl::Int] = &[
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::SURFACE_TYPE,
        egl::PBUFFER_BIT,
        egl::NONE,
    ];
    let mut configs = Vec::with_capacity(8);
    if let Err(e) = egl.choose_config(display, config_attribs, &mut configs) {
        eprintln!("skip: eglChooseConfig falló ({e:?})");
        return None;
    }
    let config = match configs.first() {
        Some(c) => *c,
        None => {
            eprintln!("skip: sin configs EGL");
            return None;
        }
    };
    // OpenGL desktop 3.3 Core (ADR-01: EGL_CONTEXT_MAJOR/MINOR_VERSION).
    if let Err(e) = egl.bind_api(egl::OPENGL_API) {
        eprintln!("skip: eglBindApi falló ({e:?})");
        return None;
    }
    let context_attribs: &[egl::Int] = &[
        egl::CONTEXT_MAJOR_VERSION,
        3,
        egl::CONTEXT_MINOR_VERSION,
        3,
        egl::NONE,
    ];
    let context = match egl.create_context(display, config, None, context_attribs) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skip: eglCreateContext falló ({e:?})");
            return None;
        }
    };
    let surface_attribs: &[egl::Int] = &[egl::WIDTH, 64, egl::HEIGHT, 64, egl::NONE];
    let surface = match egl.create_pbuffer_surface(display, config, surface_attribs) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("skip: eglCreatePbufferSurface falló ({e:?})");
            return None;
        }
    };
    if let Err(e) = egl.make_current(display, Some(surface), Some(surface), Some(context)) {
        eprintln!("skip: eglMakeCurrent falló ({e:?})");
        return None;
    }

    let ctx = match GraphicsContext::from_loader(
        |name| {
            egl.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
        },
        64,
        64,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skip: creación del GraphicsContext falló ({e})");
            return None;
        }
    };
    std::mem::forget(Headless {
        egl,
        context,
        surface,
    });
    Some(ctx)
}

/// Lee el píxel central del target en coordenadas pantalla (Y-abajo).
fn center_pixel(ctx: &GraphicsContext, x: i32, y: i32) -> [u8; 4] {
    let h = ctx.size().1 as i32;
    // Origen de read_pixels: abajo-izquierda.
    let buf = ctx.read_pixels(x, h - y - 1, 1, 1);
    [buf[0], buf[1], buf[2], buf[3]]
}

fn assert_color(actual: [u8; 4], expected: [u8; 4], tolerance: u8, what: &str) {
    for i in 0..4 {
        let diff = actual[i].abs_diff(expected[i]);
        assert!(
            diff <= tolerance,
            "{what}: canal {i} = {}, esperado ~{} (±{tolerance})",
            actual[i],
            expected[i]
        );
    }
}

#[test]
fn gpu_clear_y_rect_plano() {
    let Some(mut ctx) = make_context() else {
        return;
    };
    // Clear rojo opaco.
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::opaque(1.0, 0.0, 0.0)));
    ctx.render(&queue);
    assert_color(
        center_pixel(&ctx, 32, 32),
        [255, 0, 0, 255],
        2,
        "clear rojo",
    );

    // Rectángulo azul opaco en el centro.
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::opaque(1.0, 0.0, 0.0)));
    queue.rect(
        0,
        Rect::new(16.0, 16.0, 32.0, 32.0),
        Color::opaque(0.0, 0.0, 1.0),
    );
    ctx.render(&queue);
    assert_color(center_pixel(&ctx, 32, 32), [0, 0, 255, 255], 2, "rect azul");
    assert_color(center_pixel(&ctx, 8, 8), [255, 0, 0, 255], 2, "fondo rojo");
}

#[test]
fn gpu_texturas_y_capas() {
    let Some(mut ctx) = make_context() else {
        return;
    };
    // Textura verde 4x4.
    let tex = ctx
        .create_texture(4, 4, &[[0, 255, 0, 255]; 16].concat())
        .unwrap();
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::BLACK));
    // Capa 0: quad verde completo.
    queue.textured_rect(
        0,
        Rect::new(0.0, 0.0, 64.0, 64.0),
        tex,
        Rect::new(0.0, 0.0, 1.0, 1.0),
        Color::WHITE,
        &rivet_canvas::Transform::identity(),
    );
    // Capa 1: rect rojo pequeño encima.
    queue.rect(
        1,
        Rect::new(28.0, 28.0, 8.0, 8.0),
        Color::opaque(1.0, 0.0, 0.0),
    );
    let stats = ctx.render(&queue);
    assert!(stats.draw_calls >= 2, "dos lotes dibujados");
    assert_color(
        center_pixel(&ctx, 32, 32),
        [255, 0, 0, 255],
        2,
        "capa superior roja",
    );
    assert_color(
        center_pixel(&ctx, 8, 8),
        [0, 255, 0, 255],
        2,
        "textura verde",
    );
}

#[test]
fn gpu_blur_difumina_la_region() {
    let Some(mut ctx) = make_context() else {
        return;
    };
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::opaque(0.0, 0.0, 1.0))); // fondo azul
                                                           // Mitad izquierda roja, con borde duro.
    queue.rect(
        0,
        Rect::new(0.0, 0.0, 32.0, 64.0),
        Color::opaque(1.0, 0.0, 0.0),
    );
    // Blur del borde: píxel justo a la derecha de la frontera (32, 32).
    queue.blur(1, Rect::new(8.0, 8.0, 48.0, 48.0), 8.0);
    ctx.render(&queue);
    let mixed = center_pixel(&ctx, 33, 32);
    // Sin blur sería azul puro (255,0,0,255 tras clear azul → [0,0,255,255]);
    // con blur debe mezclar algo de rojo.
    assert!(
        mixed[0] > 40,
        "el pixel del borde debería contener rojo difuminado: {mixed:?}"
    );
    // Lejos de la frontera sigue azul.
    assert_color(
        center_pixel(&ctx, 60, 32),
        [0, 0, 255, 255],
        40,
        "lejos del borde",
    );
}

#[test]
fn gpu_frame_stats_y_present() {
    let Some(mut ctx) = make_context() else {
        return;
    };
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::BLACK));
    for i in 0..10 {
        queue.rect(0, Rect::new(i as f32 * 6.0, 0.0, 4.0, 64.0), Color::WHITE);
    }
    let stats = ctx.render(&queue);
    assert_eq!(stats.vertices, 60);
    assert_eq!(stats.batches, 1);
    assert_eq!(stats.draw_calls, 1);
    let presented = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = std::rc::Rc::clone(&presented);
    ctx.set_present_callback(Box::new(move || flag.set(true)));
    ctx.present();
    assert!(presented.get());
}

#[test]
fn gpu_circulo_y_rounded_rect() {
    let Some(mut ctx) = make_context() else {
        return;
    };
    // Círculo azul centrado sobre fondo rojo.
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::opaque(1.0, 0.0, 0.0)));
    queue.circle(0, 32.0, 32.0, 12.0, Color::opaque(0.0, 0.0, 1.0));
    ctx.render(&queue);
    assert_color(
        center_pixel(&ctx, 32, 32),
        [0, 0, 255, 255],
        2,
        "centro círculo",
    );
    // Fuera del círculo sigue rojo.
    assert_color(
        center_pixel(&ctx, 4, 4),
        [255, 0, 0, 255],
        2,
        "fuera círculo",
    );

    // Rect redondeado: esquina exterior NO pintada, centro sí.
    let mut queue = RenderQueue::new();
    queue.clear_color(Some(Color::opaque(0.0, 0.0, 0.0)));
    queue.rounded_rect(
        0,
        Rect::new(16.0, 16.0, 32.0, 32.0),
        8.0,
        Color::opaque(0.0, 1.0, 0.0),
    );
    ctx.render(&queue);
    assert_color(
        center_pixel(&ctx, 32, 32),
        [0, 255, 0, 255],
        2,
        "centro rounded",
    );
    // Esquina (17,17) está dentro del cuadrado de la esquina pero fuera del
    // arco de radio 8: debe seguir negro (con tolerancia AA).
    let corner = center_pixel(&ctx, 17, 17);
    assert!(
        corner[1] < 100,
        "esquina recortada por el radio: {corner:?}"
    );
}
