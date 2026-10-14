//! Integration tests: system fonts (DejaVu when present) and the bundled
//! Inter font (deterministic, no environment dependencies).

use rivet_typography::{FontDb, GlyphAtlas, GlyphCache, BUNDLED_FONT, BUNDLED_FONT_LICENSE};

/// Carga una fuente de prueba: DejaVu Sans si está (Linux típico, CI de
/// GitHub incluida), si no la empaquetada.
fn load_font() -> (FontDb, rivet_typography::FontId) {
    let mut db = FontDb::new();
    db.load_system();
    let id = db
        .select(&["DejaVu Sans"], fontdb::Weight::default())
        .or_else(|| db.sans_serif())
        .unwrap_or_else(|| db.load_bundled().expect("fuente empaquetada"));
    (db, id)
}

#[test]
fn metricas_y_layout_basico() {
    let (db, id) = load_font();
    let font = db.font(id, 24.0).unwrap();
    let metrics = font.metrics();
    assert!(metrics.ascent > 0.0, "ascenso positivo");
    assert!(metrics.descent >= 0.0, "descenso positivo hacia abajo");
    assert!(metrics.line_height() > metrics.ascent);
    assert_eq!(metrics.size_px, 24.0);
    assert!(metrics.units_per_em >= 500, "upem típico (1000/2048)");

    let layout = font.layout("Hola");
    assert_eq!(layout.glyphs.len(), 4);
    assert!(layout.width > 0.0);
    // Monotonía: cada glifo empieza donde termina el flujo anterior.
    for pair in layout.glyphs.windows(2) {
        assert!(
            pair[1].x >= pair[0].x + pair[0].advance - 6.0,
            "posiciones crecientes"
        );
    }
    // El espacio añade avance frente a pegar los caracteres.
    let joined = font.layout("ab");
    let with_space = font.layout("a b");
    assert!(with_space.width > joined.width);
}

#[test]
fn rasterizado_con_antialias() {
    let (db, id) = load_font();
    let font = db.font(id, 32.0).unwrap();
    let layout = font.layout("H");
    let glyph = font.rasterize(layout.glyphs[0]).expect("H con contorno");
    assert!(glyph.width >= 8 && glyph.height >= 8, "tamaño razonable");

    // Invariantes del rasterizado con AA: trazos sólidos + bordes suaves.
    let mut sorted = glyph.coverage.clone();
    sorted.sort_unstable();
    let solid = sorted.iter().filter(|&&c| c == 255).count();
    assert!(
        solid > glyph.coverage.len() / 8,
        "interior del trazo sólido"
    );
    // El píxel más oscuro del borde superior (AA) tiene cobertura intermedia.
    let partial = sorted.iter().filter(|&&c| (10..=245).contains(&c)).count();
    assert!(partial > 0, "bordes con cobertura intermedia");
    // Masa acotada (sin desbordes del algoritmo de winding).
    let mass: u32 = glyph.coverage.iter().map(|&c| c as u32).sum();
    assert!(mass > 0 && mass <= (glyph.coverage.len() * 255) as u32);
}

#[test]
fn cache_lru_con_aciertos() {
    let (db, id) = load_font();
    let font = db.font(id, 20.0).unwrap();
    let face = db.face(id).unwrap();
    let mut cache = GlyphCache::new(16);
    let layout = font.layout("ab");
    for g in &layout.glyphs {
        let _ = cache.get_or_raster(face.data(), face.index(), face.face(), 20.0, g.glyph, 0);
    }
    assert_eq!(cache.misses(), 2, "dos fallos iniciales");
    // Segunda pasada: aciertos.
    for g in &layout.glyphs {
        let _ = cache.get_or_raster(face.data(), face.index(), face.face(), 20.0, g.glyph, 0);
    }
    assert_eq!(cache.hits(), 2, "segunda pasada desde caché");
    assert_eq!(cache.len(), 2);
}

#[test]
fn atlas_empaqueta_y_convierte_a_rgba() {
    let (db, id) = load_font();
    let font = db.font(id, 24.0).unwrap();
    let mut atlas = GlyphAtlas::new(128, 1);
    let layout = font.layout("Rivet 2026");
    let mut rects = Vec::new();
    for g in &layout.glyphs {
        if let Some(bitmap) = font.rasterize(*g) {
            rects.push(atlas.alloc(&bitmap));
        }
    }
    assert!(!rects.is_empty());
    assert_eq!(atlas.page_count(), 1, "cabe en una página pequeña");
    let rgba = atlas.page_rgba(0).unwrap();
    assert_eq!(rgba.len(), 128 * 128 * 4);
    // Los rectángulos no se solapan (mismo shelf → x crecientes).
    let mut sorted = rects.clone();
    sorted.sort_by_key(|r| r.y);
    for pair in sorted.windows(2) {
        let separated = pair[0].y + pair[0].h <= pair[1].y
            || pair[0].x + pair[0].w <= pair[1].x
            || pair[1].x + pair[1].w <= pair[0].x;
        assert!(
            separated,
            "rectángulos sin solape: {:?} {:?}",
            pair[0], pair[1]
        );
    }
}

#[test]
fn cadena_de_fallback_para_caracteres() {
    let (db, id) = load_font();
    let primary = db.face(id).unwrap();
    let mut font = rivet_typography::Font::with_fallbacks(primary, vec![], 16.0);
    // 'ñ' o '€' suelen existir; un CJK símbolo raro quizá no.
    let layout = font.layout("aÿ");
    assert_eq!(layout.glyphs.len(), 2);
    // Fallback con la misma cara (una segunda instancia): sigue funcionando.
    let second = db.face(id).unwrap();
    font.push_fallback(second);
    let layout = font.layout("aÿ€中");
    assert!(!layout.glyphs.is_empty());
}

#[test]
fn fuente_empaquetada_inter_funciona() {
    // Determinista: no depende del sistema (doc §3: fallback empaquetado).
    let mut db = FontDb::new();
    let id = db.load_bundled().unwrap();
    // Idempotente.
    assert_eq!(db.load_bundled().unwrap(), id);
    assert_eq!(db.family_name(id).as_deref(), Some("Inter"));

    let font = db.font(id, 24.0).unwrap();
    let metrics = font.metrics();
    assert!(metrics.ascent > 0.0 && metrics.descent > 0.0);
    let layout = font.layout("Rivet");
    assert_eq!(layout.glyphs.len(), 5);
    assert!(layout.width > 40.0, "cinco glifos con avance");

    let glyph = font.rasterize(layout.glyphs[0]).expect("R con contorno");
    let solid = glyph.coverage.iter().filter(|&&c| c == 255).count();
    assert!(solid > glyph.coverage.len() / 8, "trazos sólidos");

    // La licencia OFL viaja con el binario (atribución).
    assert!(BUNDLED_FONT_LICENSE.starts_with(b"Copyright (c) 2016 The Inter Project"));
    assert!(BUNDLED_FONT.len() > 100_000);
}

#[test]
fn select_or_bundled_sin_sistema() {
    // Sin load_system: la selección debe caer en la empaquetada.
    let mut db = FontDb::new();
    let id = db
        .select_or_bundled(
            &["Cualquier Familia Inexistente"],
            fontdb::Weight::default(),
        )
        .unwrap();
    assert_eq!(db.family_name(id).as_deref(), Some("Inter"));
}

#[test]
fn subpixel_cuantizacion_y_rasterizado() {
    // Cuantización pura (determinista, sin fuentes).
    // (probamos vía layout con la empaquetada: bin correcto por avance fraccional)
    let mut db = FontDb::new();
    let id = db.load_bundled().unwrap();
    let font = db.font(id, 24.0).unwrap();

    // 1) Layout produce posiciones enteras + bin 0..3.
    let layout = font.layout("AVATAR");
    for g in &layout.glyphs {
        assert_eq!(g.x, g.x.floor(), "posiciones enteras");
        assert!(g.subpixel <= 3, "bin 0..=3");
    }

    // 2) Mismo glifo con bins distintos rasteriza distinto (subpíxel real).
    let base = layout.glyphs[0];
    let bin0 = font.rasterize(base).expect("contorno");
    let mut shifted = base;
    shifted.subpixel = 2;
    let bin2 = font.rasterize(shifted).expect("contorno");
    assert_eq!(
        (bin0.width, bin0.height),
        (bin2.width, bin2.height),
        "mismo bbox de referencia"
    );
    assert_ne!(
        bin0.coverage, bin2.coverage,
        "el offset subpíxel debe cambiar la máscara"
    );

    // 3) La caché separa por bin: mismo glifo, dos bins = dos entradas.
    let face = db.face(id).unwrap();
    let mut cache = GlyphCache::new(8);
    for bin in [0u8, 2u8] {
        let _ = cache.get_or_raster(
            face.data(),
            face.index(),
            face.face(),
            24.0,
            base.glyph,
            bin,
        );
    }
    assert_eq!(cache.len(), 2, "clave incluye el bin subpíxel");
}

#[test]
fn gpos_kerning_estrecha_pares() {
    // Inter declara feature `kern` en GPOS: "AV" debe quedar más estrecho
    // que la suma de sus avances (par clásico kernneado negativo).
    let mut db = FontDb::new();
    let id = db.load_bundled().unwrap();
    let font = db.font(id, 32.0).unwrap();

    let a = font.layout("A").width;
    let v = font.layout("V").width;
    let av = font.layout("AV").width;
    assert!(
        av < a + v - 0.01,
        "kerning GPOS aplicado: AV={av} vs A+V={}+{}",
        a,
        v
    );

    // "AA" también puede llevar kerning en Inter (extensivo), pero siempre
    // menor que el par clásico "AV".
    let aa = font.layout("AA").width;
    let delta_av = (a + v) - av;
    let delta_aa = (a + a) - aa;
    assert!(
        delta_av > delta_aa,
        "AV más kernneado que AA: {delta_av} vs {delta_aa}"
    );
}
