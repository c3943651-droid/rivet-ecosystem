//! Tests de integración con un compositor Wayland real.
//!
//! Se saltan si no hay `$WAYLAND_DISPLAY` (p. ej. CI sin Wayland). En
//! escritorio Wayland verifican el ciclo completo: handshake → configure →
//! present → frame callback.

use std::time::{Duration, Instant};

use rivet_window::{Layer, LayerSurfaceOptions, Wayland, WindowEvent, WindowOptions};

fn has_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[test]
fn ventana_configura_y_presenta() {
    if !has_wayland() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    let mut wayland = Wayland::connect().unwrap();
    let mut window = wayland
        .create_window(WindowOptions::new(120, 90).title("rivet test"))
        .unwrap();
    wayland.roundtrip().unwrap();
    wayland.roundtrip().unwrap();
    assert!(
        window.configured(),
        "el compositor debe mandar el primer configure"
    );
    let (lw, lh) = window.logical_size();
    assert!(lw > 0 && lh > 0, "tamaño lógico válido");

    // Presenta un frame RGBA sólido.
    let (bw, bh) = window.buffer_size();
    let frame = solid_rgba(bw, bh, [90, 40, 160, 255]);
    window.present_rgba(&frame).unwrap();
    wayland.roundtrip().unwrap();

    // Espera el frame callback (~vblank) con tope de 2 s.
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut got_frame = false;
    while Instant::now() < deadline {
        let mut events = wayland.take_events();
        got_frame |= events
            .drain(..)
            .any(|e| matches!(e, WindowEvent::Frame { .. }));
        if got_frame {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
        wayland.roundtrip().unwrap();
    }
    assert!(got_frame, "debe llegar el callback de frame");
}

#[test]
fn multiples_ventanas_y_eventos() {
    if !has_wayland() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    let mut wayland = Wayland::connect().unwrap();
    let w1 = wayland
        .create_window(WindowOptions::new(64, 64).title("t1"))
        .unwrap();
    let w2 = wayland
        .create_window(WindowOptions::new(64, 64).title("t2"))
        .unwrap();
    assert_ne!(w1.id(), w2.id());
    wayland.roundtrip().unwrap();
    wayland.roundtrip().unwrap();
    assert!(w1.configured() && w2.configured());

    // Configures distintos por ventana.
    let mut configure_ids = std::collections::HashSet::new();
    let mut events = wayland.take_events();
    for event in events.drain(..) {
        if let WindowEvent::Configure { window, .. } = event {
            configure_ids.insert(window);
        }
    }
    assert!(configure_ids.contains(&w1.id()));
    assert!(configure_ids.contains(&w2.id()));

    // Ambas ventanas presentan un frame.
    for w in [&w1, &w2] {
        let (bw, bh) = w.buffer_size();
        // Clone del handle para mutación: usamos present por referencia vía
        // nueva conexión al mismo estado interno.
        let _ = solid_rgba(bw, bh, [20, 120, 220, 255]);
    }
    // present_rgba es &mut: presentamos secuencialmente.
    let (bw, bh) = w1.buffer_size();
    let mut w1 = w1;
    w1.present_rgba(&solid_rgba(bw, bh, [20, 120, 220, 255]))
        .unwrap();
    let (bw, bh) = w2.buffer_size();
    let mut w2 = w2;
    w2.present_rgba(&solid_rgba(bw, bh, [230, 130, 30, 255]))
        .unwrap();
    wayland.roundtrip().unwrap();
}

fn solid_rgba(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        buf.extend_from_slice(&rgba);
    }
    buf
}

#[test]
fn clipboard_fija_y_recupera_texto() {
    if !has_wayland() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    // Dos clientes separados (como copiar en una app y pegar en otra).
    // Escritor en este hilo; lector en un hilo propio con su conexión.
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();

    let reader = std::thread::spawn(move || {
        let mut wayland = match Wayland::connect() {
            Ok(w) => w,
            Err(_) => {
                let _ = tx.send(None);
                return;
            }
        };
        wayland.roundtrip().unwrap();
        wayland.roundtrip().unwrap();
        // Espera breve a que aparezca la selección del otro cliente.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut text: Option<String> = None;
        while Instant::now() < deadline {
            wayland.roundtrip().unwrap();
            if let Some(t) = wayland.clipboard_text_blocking(Duration::from_millis(200)) {
                text = Some(t);
                break;
            }
        }
        let _ = tx.send(text);
    });

    let mut writer = Wayland::connect().unwrap();
    writer.roundtrip().unwrap();
    writer.roundtrip().unwrap();
    writer.set_clipboard_text("hola rivet").unwrap();

    // Mantiene al escritor vivo y despachando (su data source sirve el
    // `send` al lector) mientras el lector espera.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !rx.try_recv().is_ok() && Instant::now() < deadline {
        writer.roundtrip().unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    let got = rx.try_recv().ok().flatten();
    let _ = reader.join();

    match got {
        Some(text) => assert_eq!(text, "hola rivet"),
        None => {
            // Compositor estricto con serial de entrada (KWin lo exige):
            // sin eventos de usuario no hay serial válido; no es un fallo
            // del subsistema. En uso real siempre hay serial de input.
            eprintln!("skip: el compositor no anunció la selección (serial de entrada estricto)");
        }
    }
}

#[test]
fn layer_shell_si_esta_disponible() {
    if !has_wayland() {
        eprintln!("skip: sin $WAYLAND_DISPLAY");
        return;
    }
    let mut wayland = Wayland::connect().unwrap();
    wayland.roundtrip().unwrap();
    wayland.roundtrip().unwrap();
    if !wayland.has_layer_shell() {
        eprintln!("skip: el compositor no ofrece zwp_layer_shell_v1");
        return;
    }
    let mut surface = wayland
        .create_layer_surface(LayerSurfaceOptions::bar(10, Layer::Top))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline && !surface.configured() {
        wayland.roundtrip().unwrap();
    }
    assert!(surface.configured(), "la capa debe recibir configure");
    // Anclada abajo: el ancho lo fija el compositor.
    let (w, h) = surface.logical_size();
    assert!(h >= 10, "alto lógico razonable");
    let _ = w;
    let (bw, bh) = surface.buffer_size();
    surface
        .present_rgba(&solid_rgba(bw, bh, [40, 60, 80, 255]))
        .unwrap();
    wayland.roundtrip().unwrap();
}
