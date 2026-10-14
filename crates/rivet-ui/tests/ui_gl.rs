//! Headless GPU tests: the complete UI render pipeline (layout +
//! styles + text with atlas) over a canvas FBO.

use rivet_canvas::{GraphicsContext, RenderQueue};
use rivet_ui::layout::layout_tree;
use rivet_ui::render::render_tree;
use rivet_ui::Ui;

fn make_gl() -> Option<GraphicsContext> {
    let egl =
        unsafe { khronos_egl::DynamicInstance::<khronos_egl::EGL1_4>::load_required() }.ok()?;
    let display = unsafe { egl.get_display(khronos_egl::DEFAULT_DISPLAY) }?;
    egl.initialize(display).ok()?;
    let mut configs = Vec::with_capacity(8);
    egl.choose_config(
        display,
        &[
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
        ],
        &mut configs,
    )
    .ok()?;
    let config = *configs.first()?;
    egl.bind_api(khronos_egl::OPENGL_API).ok()?;
    let context = egl
        .create_context(
            display,
            config,
            None,
            &[
                khronos_egl::CONTEXT_MAJOR_VERSION,
                3,
                khronos_egl::CONTEXT_MINOR_VERSION,
                3,
                khronos_egl::NONE,
            ],
        )
        .ok()?;
    let surface = egl
        .create_pbuffer_surface(
            display,
            config,
            &[
                khronos_egl::WIDTH,
                64,
                khronos_egl::HEIGHT,
                64,
                khronos_egl::NONE,
            ],
        )
        .ok()?;
    egl.make_current(display, Some(surface), Some(surface), Some(context))
        .ok()?;
    let gl = GraphicsContext::from_loader(
        |name| {
            egl.get_proc_address(name)
                .map_or(std::ptr::null(), |f| f as *const core::ffi::c_void)
        },
        64,
        64,
    )
    .ok()?;
    std::mem::forget((egl, display, context, surface));
    Some(gl)
}

#[test]
fn render_completo_dibuja_fondo_boton_y_texto() {
    let Some(mut gl) = make_gl() else {
        eprintln!("skip: sin libEGL/GL");
        return;
    };
    let mut ui = Ui::new_dark();
    let root = ui.registry.create_box(true);
    ui.registry.set_root(root).unwrap();
    let label = ui.registry.create_label("Rivet");
    let button = ui.registry.create_button("OK", |_| {});
    ui.registry.add_child(root, label).unwrap();
    ui.registry.add_child(root, button).unwrap();

    let (w, h) = (256u32, 128u32);
    let (fbo, texture) = gl.create_render_target(w, h).unwrap();
    gl.resize(w, h);
    gl.set_target(Some(fbo));

    let style = std::mem::replace(&mut ui.style, rivet_ui::default_style_table());
    let mut text = std::mem::take(&mut ui.text);
    layout_tree(
        &mut ui.registry,
        &mut text,
        &style,
        root,
        w as f32,
        h as f32,
    )
    .unwrap();
    ui.text = text;
    ui.style = style;

    let mut queue = RenderQueue::new();
    queue.clear_color(Some(rivet_canvas::Color::opaque(0.0, 0.0, 0.0)));
    render_tree(
        &mut ui.registry,
        &mut ui.text,
        &ui.style,
        &mut gl,
        &mut queue,
        root,
    )
    .unwrap();
    let stats = gl.render(&queue);
    assert!(stats.draw_calls >= 2, "fondo + contenido");

    let l = ui.registry.node(label).unwrap().rect;
    let b = ui.registry.node(button).unwrap().rect;
    eprintln!("label={l:?} button={b:?}");
    gl.destroy_render_target(fbo, texture);
}
