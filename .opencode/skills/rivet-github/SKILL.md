---
name: rivet-github
description: Gestión del repositorio GitHub de Rivet (c3943651-droid/rivet-ecosystem) con gh CLI: issues, labels, milestones, pull requests, releases y visibilidad. Usar cuando el usuario hable de issues, etiquetas, hitos, PRs, releases o administrar el repo en GitHub.
---

# Gestión GitHub — Rivet

Repo: `c3943651-droid/rivet-ecosystem` (privado de momento). Usar siempre `gh`, nunca la web.

## Labels estándar

Crear una sola vez:

```bash
gh label create bug --description "Algo no funciona" --color d73a4a
gh label create enhancement --description "Nueva funcionalidad" --color a2eeef
gh label create docs --description "Solo documentación" --color 0075ca
gh label create measurement --description "Medición según §14 de RIVET.md" --color fbca04
gh label create blocked --description "Bloqueado por otra tarea" --color b60205
```

Una label por crate (`crate:foundation`, `crate:window`, …, `crate:rivet`):

```bash
for c in foundation window canvas typography image style ui rivet; do
  gh label create "crate:$c" --color c5def5
done
```

## Milestones (fases de docs/RIVET.md §13.2)

```bash
gh api repos/{owner}/{repo}/milestones -f title="Fase 0 — Especificación de crates"
gh api repos/{owner}/{repo}/milestones -f title="Fase 1 — foundation + window"
```

## Issues

```bash
gh issue create --title "..." --body "..." --label "crate:window,enhancement" --milestone "Fase 1 — foundation + window"
gh issue list --state open --limit 30
gh issue close <n> --comment "..."
```

En el body de un issue de medición incluir siempre: método, baseline y criterio (plantilla §14 de `docs/RIVET.md`).

## Pull requests

```bash
gh pr create --fill --label "crate:ui"
gh pr view --web          # o sin --web para texto
gh pr checks              # CI debe estar verde antes de fusionar
gh pr merge --squash --delete-branch
```

Reglas: 1 PR = 1 tema; si toca varios crates, justificar en la descripción; no fusionar con checks en rojo.

## Releases y visibilidad

```bash
gh release create vX.Y.Z --generate-notes
gh repo edit --visibility public        # cuando toque (pide confirmación)
gh repo view --json name,visibility
```

## Estado del repo

```bash
gh pr list --state open
gh run list --limit 5        # runs de CI
gh run watch                  # seguir el run en curso
```
