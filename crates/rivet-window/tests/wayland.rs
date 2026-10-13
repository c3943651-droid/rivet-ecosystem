//! Tests de integración con un compositor Wayland real.
//!
//! Se saltan si no hay `$WAYLAND_DISPLAY` (p. ej. CI sin Wayland). En
//! escritorio Wayland verifican el ciclo completo: handshake → configure →
//! present → frame callback.

use std::time::{Duration, Instant};

use rivet_window::{Wayland, WindowEvent, WindowOptions};

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
