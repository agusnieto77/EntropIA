# CI Rust Quality (report-first)

Este flujo agrega señal de calidad Rust sin enforcement agresivo inicial.

## Baseline contractual

- Cobertura: `cargo llvm-cov --manifest-path apps/desktop/src-tauri/Cargo.toml --no-default-features --lcov`
- Artifact principal: `apps/desktop/src-tauri/coverage-rust/lcov.info`
- Summary: `apps/desktop/src-tauri/coverage-rust/summary.md`

## Estados

| Estado        | Significado                                                                               |
| ------------- | ----------------------------------------------------------------------------------------- |
| `pass`        | El comando ejecutó correctamente y cumplió el objetivo esperado.                          |
| `report-only` | Hay deuda de código (fmt/clippy) reportada, pero no error de infraestructura.             |
| `infra-error` | Falló la invocación/tooling (por ejemplo, falta `cargo-llvm-cov` o `llvm-tools-preview`). |

## Criterio verify para `.rs`

- Si el changeset toca archivos `.rs`, se debe publicar `rust-verify-evidence.md` con:
  1. Cobertura baseline (`--no-default-features`)
  2. Reporte de quality (`fmt`/`clippy`)
  3. Clasificación por señal (`pass | report-only | infra-error`)
- Si NO hay cambios `.rs`, verify puede omitir evidencia Rust dejando justificación `out-of-scope`.

## No-enforcement inicial

- El job `rust-quality-report` se ejecuta en modo report-first (`continue-on-error: true`).
- La deuda histórica de `fmt`/`clippy` no bloquea PR en esta fase.
- Los `infra-error` se exponen explícitamente en summary/artifacts para mantenimiento.
