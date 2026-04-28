# Análisis de optimizaciones (2026-04-28)

## 1) Cuellos de botella de concurrencia en workers

### Observación
- OCR procesa trabajos en un loop serial y, además, abre una conexión SQLite nueva en cada guardado de extracción.
- NLP también drena la cola serialmente y en `EnrichItem` ejecuta FTS → embeddings → NER → triples en secuencia.
- Transcripción procesa en un único worker secuencial.

### Optimización propuesta
- Mantener 1 conexión SQLite por worker (como ya hace NLP/Transcripción) para OCR.
- Introducir concurrencia acotada por tipo de tarea (p. ej. semáforos):
  - OCR: 1–2 jobs en paralelo (CPU-bound, configurable).
  - Embeddings/transcripción: hasta N procesos Python concurrentes con cola priorizada.
- Separar colas por clase de trabajo para evitar head-of-line blocking:
  - `quick` (FTS/indexado), `heavy-cpu` (OCR/NER/triples), `python` (whisper/fastembed/PaddleVL).

### Impacto esperado
- Menor latencia percibida cuando entran lotes mixtos.
- Mejor throughput total sin saturar CPU por sobre-paralelismo.

---

## 2) Escrituras SQLite: patrón delete+insert

### Observación
- En OCR y transcripción se aplica semántica upsert borrando por `asset_id` y luego insertando.

### Optimización propuesta
- Cambiar a `INSERT ... ON CONFLICT(asset_id) DO UPDATE`.
- Para habilitarlo, crear índice/constraint único por `asset_id` en `extractions` y `transcriptions`.
- Usar transacción explícita para “persistir extracción + lookup item_id + enqueue de NLP” como unidad.

### Impacto esperado
- Menor churn en índices/fragmentación y menos I/O.
- Menor riesgo de ventana inconsistente entre delete e insert.

---

## 3) Embeddings por asset sobrescriben embeddings por item

### Observación
- El embedding de asset hoy se guarda con clave `item_id`, reemplazando el embedding global del item.

### Optimización propuesta
- Crear tabla `vec_assets(asset_id PRIMARY KEY, item_id, embedding BLOB)`.
- Mantener `vec_items` para embedding global y `vec_assets` para granularidad por página/asset.
- Ajustar búsqueda de similares para soportar ambos niveles.

### Impacto esperado
- Evita regresiones de calidad semántica en similitud por item.
- Habilita features nuevas (similitud por página).

---

## 4) Coste de spawn de Python por invocación

### Observación
- Whisper y fastembed crean subprocess nuevo por llamada.

### Optimización propuesta
- Implementar sidecar persistente opcional (ya existe `tools/llm-sidecar` como precedente de arquitectura distribuida en repo).
- Protocolo simple stdin/stdout JSONL con healthcheck y reinicio automático.
- Mantener modo actual como fallback.

### Impacto esperado
- Reducción fuerte de latencia en cargas repetitivas (warm model + warm interpreter).

---

## 5) Migraciones dispersas en setup de `lib.rs`

### Observación
- Parte de la evolución de esquema se ejecuta en runtime startup (checks + ALTER TABLE + CREATE INDEX).

### Optimización propuesta
- Mover toda mutación de esquema a migraciones versionadas del package `store`.
- Dejar en `setup` sólo validaciones de salud (sin DDL).
- Añadir “schema contract test” que compare estado final de DB con schema esperado.

### Impacto esperado
- Startup más predecible, menor complejidad operativa y menos riesgo de drift.

---

## 6) IPC SQL genérico (coste y superficie)

### Observación
- Existen comandos IPC que aceptan SQL arbitrario + parámetros JSON y convierten dinámicamente tipos.

### Optimización propuesta
- Introducir capa de comandos tipados para operaciones frecuentes (select/update concretos).
- Conservar SQL genérico sólo para casos avanzados y behind-flag en modo dev.
- Reutilizar statements preparados en operaciones hot path.

### Impacto esperado
- Menor overhead de serialización/deserialización.
- Mejor mantenibilidad y reducción de superficie de error.

---

## 7) Bundle y duplicación de scripts

### Observación
- Coexisten `scripts/*.py` y `resources/scripts/*.py`, y además se incluye `resources/*` y `scripts/*` en bundle.

### Optimización propuesta
- Definir única fuente de verdad (ideal: `scripts/`), y copiar a recursos en build step determinista.
- Reducir patrones de `resources` en `tauri.conf.json` para evitar empaquetado redundante.

### Impacto esperado
- Menor tamaño de binario/instalador y menos drift entre copias.

---

## 8) CI y pipelines Turbo

### Observación
- `typecheck` depende de `^build` y `test` depende de `^build`.

### Optimización propuesta
- Revisar granularidad de dependencias:
  - `typecheck` debería depender de `^typecheck` o de outputs mínimos, no build completo.
  - `test` en paquetes TS puede ejecutarse sin build total si hay transpile on-the-fly.
- Activar cache remota de Turbo en CI para acelerar PRs.

### Impacto esperado
- Menor tiempo de feedback en desarrollo y CI.

---

## Prioridad sugerida (ROI)

1. **P0**: OCR DB connection persistente + upsert real + índices únicos por `asset_id`.
2. **P0**: Corregir modelo de embeddings para asset (`vec_assets`).
3. **P1**: Desacoplar colas/semáforos por tipo de trabajo.
4. **P1**: Consolidar migraciones fuera de `setup`.
5. **P2**: Sidecar Python persistente (feature flag).
6. **P2**: Optimización de bundle y pipeline Turbo.

## KPIs recomendados para medir mejora

- OCR p95: tiempo desde enqueue hasta `ocr:complete`.
- NLP p95 por job (`fts`, `embed`, `ner`, `triples`).
- Tiempo de startup “app launch → primera interacción”.
- Throughput lote: assets procesados/minuto.
- Tamaño instalador y tiempo CI end-to-end.
