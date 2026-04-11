# Proposal: Tripletes semánticos v1

## Intent

EntropIA ya tiene OCR, NER, embeddings y panel Analysis, pero todavía no expone relaciones explícitas entre entidades. Este cambio agrega extracción S|P|O por ítem para habilitar lectura relacional mínima sin introducir complejidad de grafo.

## Scope

### In Scope

- Extraer tripletes rule-based en Rust desde el texto extraído del ítem.
- Persistir tripletes en SQLite con migración y FK a `items`.
- Exponer `TripleRepo` en TypeScript (`packages/store`) con `findByItemId`, `replaceByItemId`.
- Agregar acción de extracción y lista simple Subject | Predicate | Object en `ItemView` (panel Analysis).

### Out of Scope

- Grafo de relaciones, visualización nodal o navegación entre documentos.
- Extracción cross-document o normalización ontológica.
- Modelo ML/LLM para extracción semántica.

## Capabilities

### New Capabilities

- `semantic-triples`: extracción, almacenamiento y consulta de tripletes S|P|O por ítem.

### Modified Capabilities

- `data-store`: nueva tabla `triples` + migración versionada + repositorio TS.
- `nlp-ux`: nuevo flujo de extracción de tripletes y render de lista S|P|O en Analysis.

## Approach

Implementar un módulo `nlp::triples` (regex/patrones verbales simples), ejecutar por comando Tauri en el flujo NLP existente y reemplazar tripletes previos del ítem en cada corrida (idempotencia funcional). El frontend reutiliza `NlpStore` y carga desde repo para mostrar tabla/lista mínima.

## Affected Areas

| Area                                     | Impact       | Description                                     |
| ---------------------------------------- | ------------ | ----------------------------------------------- | --- | ------------- |
| `apps/desktop/src-tauri/src/nlp/`        | Modified/New | `commands.rs`, `mod.rs`, nuevo `triples.rs`     |
| `apps/desktop/src-tauri/src/lib.rs`      | Modified     | Registrar comando Tauri de tripletes            |
| `packages/store/src/schema.ts`           | Modified     | Definir tabla `triples`                         |
| `packages/store/src/migrations/`         | New          | Migración para `triples` + índice por `item_id` |
| `packages/store/src/repos/`              | New/Modified | `triple.repo.ts` + export público               |
| `apps/desktop/src/lib/nlp.ts`            | Modified     | Cliente invoke para extracción de tripletes     |
| `apps/desktop/src/views/ItemView.svelte` | Modified     | Botón y lista S                                 | P   | O en Analysis |

## Risks

| Risk                                | Likelihood | Mitigation                                               |
| ----------------------------------- | ---------- | -------------------------------------------------------- |
| Baja precisión en textos históricos | High       | Reglas acotadas v1 + confidence + reemplazo simple       |
| Ambigüedad de predicados            | Med        | Catálogo inicial de verbos/patrones y tests con fixtures |
| Costo de mantenimiento de reglas    | Med        | Separar reglas por módulo y documentar criterios         |

## Rollback Plan

1. Quitar comando/UI de tripletes.
2. Revertir `TripleRepo` y cambios de schema.
3. No ejecutar migración en nuevas instalaciones; en instalaciones existentes, dejar tabla sin uso (sin afectar datos principales).

## Dependencies

- Requiere pipeline NLP y `ItemView` Analysis ya existentes.

## Success Criteria

- [ ] Existe comando Tauri para extraer tripletes por `item_id`.
- [ ] Tabla `triples` persiste filas S|P|O con índice por `item_id`.
- [ ] `TripleRepo` permite reemplazar y listar tripletes por ítem.
- [ ] `ItemView` muestra lista S|P|O luego de ejecutar extracción.
- [ ] Re-ejecutar extracción reemplaza resultados previos del ítem.
