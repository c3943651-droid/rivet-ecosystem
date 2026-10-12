---
name: rivet-automation
description: Automatizaciones de Rivet: verificación del workspace (check, fmt, clippy, test), CI con GitHub Actions y publicación en crates.io. Usar cuando el usuario pida verificar, formatear, correr tests, crear o modificar workflows de CI, releases o publicar crates.
---

# Automatización — Rivet

## Verificación (ejecutar siempre en este orden)

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**Regla:** ningún commit/PR se fusiona si falla cualquiera de los anteriores. La CI (`ci.yml`) aplica `fmt + clippy + test` en `ubuntu-latest`; Rivet es Wayland/OpenGL y **no compila en Windows/macOS**, así que la CI no incluye esas plataformas.

## CI (GitHub Actions)

Workflow en `.github/workflows/ci.yml` (ya creado):

- **Trigger:** `push` y `pull_request` a `main`.
- **Pasos:** checkout → toolchain stable → `cargo fmt --all --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace`.
- Para añadir pasos, editar ese archivo y validar localmente con `act` o revisando la sintaxis YAML.

## Versión y publicación (crates.io)

- Versión **unificada** del workspace: se bumpa `version` en `[workspace.package]` del `Cargo.toml` raíz y se hereda en las 8 crates.
- Semver: hasta `0.1.x` solo bumps de patch/minor sin garantías; al llegar a `1.0` se congela la API pública.
- Antes de publicar: verificación completa + `cargo publish --dry-run -p <crate>`.
- **Orden obligatorio** (dependencias path → crates.io requiere que la dependencia exista primero):

```bash
cargo publish -p rivet-foundation
cargo publish -p rivet-window
cargo publish -p rivet-canvas
cargo publish -p rivet-typography
cargo publish -p rivet-image
cargo publish -p rivet-style
cargo publish -p rivet-ui
cargo publish -p rivet        # fachada, la última
```

Esperar ~1 min entre publicaciones (propagación del índice de crates.io).

## Releases (GitHub)

```bash
# tras bump de versión en el workspace Cargo.toml:
git tag vX.Y.Z && git push origin vX.Y.Z
gh release create vX.Y.Z --generate-notes --title "vX.Y.Z"
```

Las notas de release deben citar **solo mediciones publicadas** según el protocolo del §14 de `docs/RIVET.md` (método + baseline + fecha); objetivos van marcados como objetivos.

## Medición de rendimiento

Al implementar un componente medible (§14 de `docs/RIVET.md`):

```bash
cargo bench -p <crate>        # con criterio
hyperfine '<binario>'         # micro-benchmarks CLI
perf stat -e cycles,instructions target/debug/<bin>
```

Publicar la cifra solo con el formato obligatorio de `docs/RIVET.md` §14.1.
