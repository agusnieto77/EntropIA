# Tasks: Tripletes semánticos v1

## Phase 1: Data Store Foundation (TDD)

- [x] 1.1 **RED** `packages/store/src/repos/triple.repo.test.ts`: cubrir `findByItemId` filtrando por `item_id` y `replaceByItemId` reemplazando solo el ítem objetivo (escenarios data-store).
- [x] 1.2 **GREEN** `packages/store/src/schema.ts`: agregar tabla `triples` (`id`, `item_id`, `subject`, `predicate`, `object`, `created_at`) con FK a `items`.
- [x] 1.3 **GREEN** `packages/store/src/migrations/0006_triples.sql`: crear tabla + índice `triples_item_id_idx` sobre `item_id` y comportamiento seguro en rerun.
- [x] 1.4 **GREEN** `packages/store/src/repos/triple.repo.ts`: implementar `findByItemId(itemId)` y `replaceByItemId(itemId, triples)` en transacción (delete+insert).
- [x] 1.5 **REFACTOR/WIRING** `packages/store/src/repos/store.ts` y `packages/store/src/index.ts`: exponer `triples: TripleRepo` y tipos públicos.

## Phase 2: Rust Extractor + Queue/Command (TDD)

- [x] 2.1 **RED** `apps/desktop/src-tauri/src/nlp/triples.rs`: tests unitarios de extractor rule-based (texto con tripletes, texto vacío, sin matches).
- [x] 2.2 **GREEN** `apps/desktop/src-tauri/src/nlp/triples.rs`: implementar extracción S|P|O y `extract_and_store(&conn, &item_id)` con reemplazo por ítem.
- [x] 2.3 **RED** `apps/desktop/src-tauri/src/nlp/commands.rs` (test módulo): validar enqueue de nuevo comando `extract_triples` y error de cola propagado.
- [x] 2.4 **GREEN** `apps/desktop/src-tauri/src/nlp/mod.rs`: agregar `NlpJob::ExtractTriples`, rama worker con eventos `nlp:progress|complete|error` job=`triples`, y `pub mod triples;`.
- [x] 2.5 **GREEN/WIRING** `apps/desktop/src-tauri/src/nlp/commands.rs` y `apps/desktop/src-tauri/src/lib.rs`: crear comando Tauri `extract_triples` y registrarlo en `invoke_handler`.

## Phase 3: Frontend Wrapper + ItemView (TDD)

- [x] 3.1 **RED** `apps/desktop/src/lib/nlp.test.ts`: test de wrapper `extractTriples(itemId)` invocando `invoke('extract_triples', { itemId })`.
- [x] 3.2 **GREEN** `apps/desktop/src/lib/nlp.ts`: extender `NlpJobType`/`ItemNlpState` con `triples` y agregar `extractTriples`.
- [x] 3.3 **RED** `apps/desktop/src/views/ItemView.svelte` (+ test si existe harness): escenario Analysis con estado running/error y lista vacía/llena de tripletes.
- [x] 3.4 **GREEN** `apps/desktop/src/views/ItemView.svelte`: botón “Extract Triples”, carga `store.triples.findByItemId(itemId)` y render simple Subject | Predicate | Object con empty state explícito.

## Phase 4: Integration Verification

- [x] 4.1 `packages/store/src/runner.test.ts`: asegurar aplicación de migración `0006_triples.sql` y rerun idempotente.
- [x] 4.2 `packages/store/src/repos/store.test.ts`: validar que `initStore()` devuelve `triples` repo operativo.
- [x] 4.3 `apps/desktop/src/lib/nlp.test.ts` + `ItemView` tests: verificar transición pending→running→done/error para job `triples` y retry sin bloquear UI.
