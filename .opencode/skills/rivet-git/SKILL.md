---
name: rivet-git
description: Control de versiones del workspace Rivet (rivet-ecosystem). Usar cuando el usuario mencione ramas, commits, push, pull, merge, deshacer cambios, historial, sincronizar o cualquier operación Git en Rivet. Define nombres de rama, commits convencionales y qué se versiona.
---

# Control de versiones — Rivet

- **Repo:** `github.com/c3943651-droid/rivet-ecosystem` (local: `/home/carlos/Proyectos/Rust/rivet-ecosystem`)
- **Rama base:** `main`. Todo cambio no trivial entra por PR.
- **Documentación autoritativa:** `docs/RIVET.md` (no consultarse carpetas históricas).

## Qué se versiona

- Código en `crates/*`, docs, CI y configuración.
- `/target` siempre ignorado (`.gitignore`).
- `Cargo.lock`: **no** se versiona mientras el workspace solo contenga librerías. Cuando exista un binario, se versiona y se descomenta en `.gitignore`.
- Nunca commitear secretos ni tokens.

## Ramas

Formato: `<tipo>/<crate>-<descripcion>` (o `<tipo>/<descripcion>` si es transversal).

| Tipo | Uso |
|---|---|
| `feat/` | Nueva funcionalidad |
| `fix/` | Corrección de bug |
| `chore/` | Mantenimiento, CI, dependencias |
| `docs/` | Solo documentación |
| `exp/` | Experimentos desechables |

Ejemplos: `feat/window-layer-shell`, `fix/canvas-fbo-leak`, `chore/ci-clippy`.

## Commits

Conventional Commits con **scope = crate afectada**:

```
feat(window): implementar xdg_shell básico
fix(canvas): corregir leak de FBO en blur ping-pong
chore(ci): añadir clippy con -D warnings
docs: actualizar mapa de crates en RIVET.md
```

Reglas:
1. **Verificar antes de commitear:** `cargo check --workspace` (y `cargo fmt --all` si hay tiempo).
2. Un commit nunca debe dejar el workspace sin compilar.
3. Cambios que afectan a varias crates: scope de la crate dominante o sin scope.

## Flujo estándar

```bash
git switch -c feat/<rama>
# ... cambios ...
cargo check --workspace && git add -A && git commit -m "feat(<crate>): ..."
git push -u origin feat/<rama>
gh pr create --fill
# tras aprobación y CI verde:
gh pr merge --squash --delete-branch
```

- **Merge con squash** para mantener `main` lineal.
- Si el PR toca decisiones cerradas de `docs/RIVET.md` (ADRs), citar la sección y abrir nueva entrada ADR si aplica.

## Deshacer

| Situación | Comando |
|---|---|
| Descartar cambios no commiteados | `git restore <archivo>` |
| Descartar todo el directorio | `git reset --hard origin/main` |
| Deshacer último commit (mantener cambios) | `git reset --soft HEAD~1` |
| Deshacer commit ya pusheado | `git revert <sha>` |

## Prohibido

- `git push --force` a `main` (usar `--force-with-lease` solo en ramas propias).
- Commits directos a `main` para código (docs triviales sí).
