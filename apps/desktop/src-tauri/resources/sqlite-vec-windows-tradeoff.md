# sqlite-vec Windows tradeoff (temporary)

## Problema

En Windows/MSVC, `cargo test` con default features falla al compilar `sqlite-vec v0.1.10-alpha.3` porque el crate referencia `sqlite-vec-diskann.c` pero ese archivo no está empaquetado en crates.io.

## Decisión

Aplicar un override por target Windows en `Cargo.toml` para usar un shim local `vendor/sqlite-vec` con la API mínima (`load`) que retorna éxito y permite que el pipeline degrade de forma no fatal cuando `vec_items` no está disponible.

## Tradeoffs

- **Pro**: destraba `cargo test` default-features en Windows sin cambiar el gating `embeddings` ni tocar funcionalidad NLP fuera del boundary de build.
- **Contra**: en Windows no se carga la extensión `sqlite-vec` real mientras esté activo el shim; embeddings/vector table quedan en degradación controlada.
- **Scope guard**: el comportamiento funcional de FTS/NER/triples no cambia.

## Rollback

1. Remover el override de Windows en `apps/desktop/src-tauri/Cargo.toml`.
2. Eliminar `apps/desktop/src-tauri/vendor/sqlite-vec`.
3. Volver a `sqlite-vec = "0.1.10-alpha.3"` (o versión upstream corregida) para todos los targets.
4. Reejecutar contrato en Windows (`windows-feature-contract.ps1`) para validar salida del workaround.
