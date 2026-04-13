# 🧠 EntropIA

Análisis inteligente de fuentes históricas.

---

## 🎯 Descripción

EntropIA es una aplicación desktop para el análisis de fuentes históricas digitalizadas mediante pipelines de inteligencia artificial.

A diferencia de herramientas tradicionales de gestión documental, EntropIA no se limita a almacenar o visualizar documentos: produce capas de interpretación sobre materiales inherentemente fragmentarios, incompletos o degradados.

El sistema combina:

- Gestión documental local (offline-first)
- Procesamiento automático (OCR, NER, layout)
- Análisis semántico (tripletes, embeddings, grafos)
- Sincronización en la nube (self-hosted o edge) **planificada**

---

## 🚀 Diferenciales clave

- OCR offline/local en desktop (implementado)
- Named Entity Recognition (personas, lugares, fechas, instituciones) (implementado)
- Embeddings + búsqueda semántica/FTS5 (implementado)
- Extracción de tripletes semánticos (S-P-O) (implementado, motor rule-based MVP)
- RAG sobre corpus documental (planificado)
- Exportación a herramientas externas:
  - Tropy
  - Recogito
  - Gephi
  - Voyant Tools

---

## 🏗️ Arquitectura

```
Desktop App (Tauri 2)
│
├── UI (Svelte)
├── Local DB (SQLite + Drizzle)
├── File System (imágenes/PDFs)
│
├── AI Pipeline (Adapters + Job Queue)
│   ├── OCR
│   ├── NER
│   ├── Embeddings (sqlite-vec)
│   └── Semantic Extraction
│
└── Sync Layer (roadmap)
    ├── PocketBase (self-hosted, default)
    └── Cloudflare (D1 + R2 + Workers, escala)
```

---

## ⚙️ Stack tecnológico

| Capa           | Tecnología                        | Justificación                                |
| -------------- | --------------------------------- | -------------------------------------------- |
| Desktop        | Tauri 2                           | Ligero, acceso nativo FS                     |
| Frontend       | Svelte                            | Mejor perf en desktop, bundle mínimo         |
| DB local       | SQLite                            | Offline-first                                |
| ORM            | Drizzle                           | Compatible con SQLite + D1                   |
| Vector search  | sqlite-vec                        | Offline-first, sin dependencias externas     |
| Job queue      | In-process (SQLite)               | Simple, offline-first, sin overhead          |
| AI runtime     | OCRS + fastembed + NLP rule-based | Offline-first, sin depender de APIs externas |
| Sync (roadmap) | PocketBase / Cloudflare (D1 + R2) | Objetivo de Fase 4 (aún no implementado)     |

---

## 🤖 Pipeline de IA

Arquitectura basada en adapters con job queue integrado:

```ts
interface AIProcessor {
  process(input: DocumentInput): Promise<AIResult>
}

interface Job {
  id: string
  type: 'ocr' | 'ner' | 'embeddings' | 'triples'
  status: 'pending' | 'running' | 'done' | 'error'
  assetId: string
  createdAt: Date
  result?: AIResult
  error?: string
}
```

---

## 📊 Modelo de datos (simplificado)

```sql
collections  (id TEXT PRIMARY KEY, name TEXT)
items        (id TEXT PRIMARY KEY, title TEXT, collection_id TEXT, created_at DATETIME)
assets       (id TEXT PRIMARY KEY, item_id TEXT, path TEXT, type TEXT)
notes        (id TEXT PRIMARY KEY, item_id TEXT, content TEXT)

-- AI outputs
extractions  (id TEXT PRIMARY KEY, asset_id TEXT, text_content TEXT, method TEXT, confidence REAL)
entities     (id TEXT PRIMARY KEY, item_id TEXT, entity_type TEXT, value TEXT, offsets, confidence)
triples      (id TEXT PRIMARY KEY, item_id TEXT, subject TEXT, predicate TEXT, object TEXT)
fts_items    (FTS5 virtual table: item_id, title, metadata, extracted_text)
vec_items    (sqlite-vec virtual table runtime, cuando extensión está disponible)

-- Pipeline
jobs         (id TEXT PRIMARY KEY, type TEXT, status TEXT, asset_id TEXT, created_at DATETIME, result JSON, error TEXT)
```

---

## 🔄 Cloud Sync (roadmap — aún no implementado)

### Opción 1 — PocketBase _(objetivo evaluado)_

- Self-hosted simple
- Sincronización REST + realtime
- Sin infraestructura adicional
- **Estado**: no hay capa de sync activa en el código actual

### Opción 2 — Cloudflare _(objetivo de escala)_

- D1 → metadata (SQLite serverless)
- R2 → almacenamiento de archivos
- Workers → API sync
- Durable Objects → colaboración / conflictos
- **Estado**: opción arquitectónica futura, no implementada en este repo

### Features clave

- Offline-first (funciona sin red)
- Sincronización incremental (**planificada**)
- Resolución de conflictos (last-write-wins → CRDTs) (**planificada**)
- Versionado de documentos (**planificado**)

---

## 📁 Estructura del proyecto

```
entropia/
  apps/
    desktop/          ← Tauri 2 shell
  packages/
    ui/               ← Design system (Svelte)
    store/            ← SQLite + Drizzle + repos + migrations
    config-ts/        ← tsconfig compartidos
  openspec/           ← specs, cambios y archivos SDD

  # Nota: OCR, NER, embeddings y colas de trabajo viven hoy en
  # apps/desktop/src-tauri/ (backend Rust), no en packages separados.
```

---

## 📌 Estado actual

> Última revisión manual del repo: **2026-04-13**

- ✅ **Fase 0 completada** — fundaciones del monorepo, Tauri, SQLite/Drizzle, UI base, CI.
- ✅ **Fase 1 completada** — importación documental, CRUD, viewer, metadata, notas, búsqueda y export JSON.
- ⚠️ **Fase 2 parcialmente completada** — OCR + job queue + persistencia + progreso visibles; faltan overlay OCR y algunos diferidos técnicos.
- ⚠️ **Fase 3 parcialmente completada** — NER + embeddings + FTS5 + tripletes S-P-O + viewer de entidades; falta vista de relaciones y madurez del extractor semántico.
- ⏳ **Fase 4 pendiente** — sincronización.
- ⏳ **Fase 5 pendiente** — knowledge graph / RAG / visualizaciones avanzadas.

### Diferidos conocidos

- Fase 2.5: fallback para PDFs escaneados + panel de texto completo OCR.
- Fase 3/4: vista de relaciones entre entidades y evolución del extractor de tripletes más allá del baseline rule-based.

### Fuente de verdad del avance

Este README resume el estado general, pero el detalle verificable vive en:

- `openspec/changes/archive/fase-0-fundaciones/archive-report.md`
- `openspec/changes/archive/fase-1-mvp-documental/archive-report.md`
- `openspec/changes/archive/2026-04-11-fase-2-ocr-procesamiento/archive-report.md`
- `openspec/changes/archive/2026-04-11-fase-3-nlp-embeddings-fts5/archive-report.md`

### Implementado hoy (alto nivel)

- Desktop app con Tauri 2 + Svelte 5
- SQLite + Drizzle + migrations + repositorios
- Importación documental, CRUD, viewer, metadata, notas y export JSON
- OCR offline con cola de trabajos y persistencia local
- FTS5, NER, embeddings, extracción de tripletes (S-P-O) y viewer de entidades
- Panel de tripletes en ItemView con estados de ejecución (`pending/running/done/error`)
- CI en GitHub Actions con jobs de `lint`, `typecheck`, `test`, `build` (paquetes TS/Svelte) y contrato Rust en Windows

### Desktop Rust feature contract

- **Windows (default-features)**: compila/testea sin exigir linkeo ORT; embeddings degrada de forma no fatal.
- **Windows (`--no-default-features`)**: baseline de seguridad para validar pipeline no-embedding.
- **Windows (`--features embeddings`)**: chequeo diagnóstico no bloqueante para visibilidad del path opt-in.
- **No-Windows (default-features)**: mantiene `fastembed` upstream + `sqlite-vec` upstream sin cambios funcionales.

No new NLP capability introduced by this change: no new extraction model, ranking logic, or semantic feature is added.

Rollback trigger: si vuelve a fallar el contrato default de Windows por linker ORT, revertir el target-gating de `fastembed`/features en `apps/desktop/src-tauri/Cargo.toml` al estado previo.
Rollback decision evidence MUST citar: salida de `apps/desktop/src-tauri/scripts/windows-feature-contract.ps1` (default/no-default) + pruebas de continuidad NLP no-embedding (`nlp::tests` y `nlp::commands::tests`).

---

## 🧩 Roadmap de desarrollo

### Fase 0 — Fundaciones ✅

> **Goal**: infraestructura técnica que no bloquee ninguna fase posterior.

- [x] Monorepo configurado (PNPM workspaces + Turborepo)
- [x] Tauri 2 + Svelte con hot reload
- [x] SQLite + Drizzle: schema base + migrations
- [x] `packages/ui`: design system mínimo (tokens, componentes base)
- [x] CI de calidad (lint + typecheck + test + build TS + verificación de contrato Rust en Windows)

**Done when**: app desktop vacía corre, DB migra, CI verde.

---

### Fase 1 — MVP Documental ✅

> **Goal**: validar el flujo de trabajo documental básico sin IA.

- [x] Importación de imágenes y PDFs (drag & drop + file picker)
- [x] Gestión de colecciones (CRUD)
- [x] Visor de documentos (image/PDF)
- [x] Metadata editable
- [x] Notas por ítem
- [x] Búsqueda por texto en metadata
- [x] Exportación básica (JSON)

**Done when**: un historiador puede gestionar una colección de 100 documentos sin IA.

---

### Fase 2 — AI Pipeline Core ⚠️ Parcial

> **Goal**: infraestructura de IA que soporte todos los procesadores futuros.

- [ ] Interfaz `AIProcessor` implementada como paquete independiente
- [x] Job queue (pending / running / done / error)
- [x] OCR adapter implementado (motor local/offline en Rust; el diseño evolucionó respecto al README original)
- [x] Procesamiento con progreso visible por documento
- [x] Almacenamiento de resultados OCR en base local
- [ ] Overlay de texto sobre imagen
- [ ] Layout analysis (bloques, columnas)

**Done when**: se pueden OCR-ear 50 documentos en batch y ver el resultado superpuesto.

> Estado real: OCR y cola existen, pero el overlay y el batch/UX más completo siguen pendientes.

---

### Fase 3 — Semántica ⚠️ Parcial

> **Goal**: extraer conocimiento estructurado del texto. Requiere Fase 2.

- [x] NER (personas, lugares, fechas, organizaciones)
- [x] Extracción de tripletes S-P-O (baseline rule-based)
- [x] Viewer de entidades por documento
- [x] Embeddings por documento/chunk (sqlite-vec)
- [x] Búsqueda semántica por similitud vectorial
- [ ] Vista de relaciones entre entidades

**Done when**: dado un corpus, se pueden encontrar documentos semánticamente relacionados y ver entidades extraídas.

> Estado real: NER, FTS5, embeddings y tripletes ya están; falta cerrar relaciones y robustecer extracción semántica para dar la fase por completa.

---

### Fase 4 — Sync

> **Goal**: multi-dispositivo sin pérdida de datos. Puede correr en paralelo con Fase 3.

- [ ] Schema de sync (qué se sincroniza, qué queda local)
- [ ] Sincronización incremental de metadata
- [ ] Sincronización de assets
- [ ] Resolución de conflictos básica (last-write-wins)
- [ ] Versionado de documentos
- [ ] Migración PocketBase → Cloudflare documentada

**Done when**: dos dispositivos convergen al mismo estado sin pérdida de datos.

---

### Fase 5 — Knowledge Graph + Avanzado

> **Goal**: herramientas de análisis de alto nivel.

- [ ] Grafo de relaciones (visualización interactiva)
- [ ] RAG sobre corpus (chat con los documentos)
- [ ] Timeline de eventos
- [ ] Mapas geográficos de entidades
- [ ] Exportación a Tropy / Recogito / Gephi / Voyant Tools

**Done when**: un historiador puede explorar su corpus como grafo y hacer preguntas en lenguaje natural.

---

## 📌 Estado

🚧 En desarrollo activo — fundaciones + MVP documental + OCR/NLP base ya entregados.

### Próximo paso recomendado

- Actualizar este README a medida que se cierren diferidos.
- Elegir uno de estos frentes para el próximo change grande:
  - **Fase 2.5**: fallback para PDFs escaneados + texto OCR completo
  - **Semántica avanzada**: tripletes S-P-O + relaciones
  - **Fase 4 sync**: sincronización multi-dispositivo
