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
- Sincronización en la nube (self-hosted o edge)

---

## 🚀 Diferenciales clave

- OCR avanzado vía APIs (olmOCR, Gemini)
- Layout analysis (bloques, columnas, tablas)
- Named Entity Recognition (personas, lugares, fechas)
- Extracción de tripletes semánticos (S-P-O)
- Embeddings + búsqueda semántica
- RAG sobre corpus documental
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
└── Sync Layer
    ├── PocketBase (self-hosted, default)
    └── Cloudflare (D1 + R2 + Workers, escala)
```

---

## ⚙️ Stack tecnológico

| Capa            | Tecnología                | Justificación |
|-----------------|--------------------------|--------------|
| Desktop         | Tauri 2                  | Ligero, acceso nativo FS |
| Frontend        | Svelte                   | Mejor perf en desktop, bundle mínimo |
| DB local        | SQLite                   | Offline-first |
| ORM             | Drizzle                  | Compatible con SQLite + D1 |
| Vector search   | sqlite-vec               | Offline-first, sin dependencias externas |
| Job queue       | In-process (SQLite)      | Simple, offline-first, sin overhead |
| AI APIs         | DeepInfra / Gemini       | OCR + NLP |
| Sync (default)  | PocketBase               | Self-hosting simple, migrable |
| Sync (escala)   | Cloudflare (D1 + R2)     | Edge + SQLite-compatible |

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
ocr_results  (id TEXT PRIMARY KEY, asset_id TEXT, text TEXT, layout JSON)
entities     (id TEXT PRIMARY KEY, name TEXT, type TEXT)
relations    (id TEXT PRIMARY KEY, subject_id TEXT, predicate TEXT, object_id TEXT)
embeddings   (id TEXT PRIMARY KEY, ref_id TEXT, vector BLOB)

-- Pipeline
jobs         (id TEXT PRIMARY KEY, type TEXT, status TEXT, asset_id TEXT, created_at DATETIME, result JSON, error TEXT)
```

---

## 🔄 Cloud Sync

### Opción 1 — PocketBase *(default)*
- Self-hosted simple
- Sincronización REST + realtime
- Sin infraestructura adicional

### Opción 2 — Cloudflare *(escala)*
- D1 → metadata (SQLite serverless)
- R2 → almacenamiento de archivos
- Workers → API sync
- Durable Objects → colaboración / conflictos

### Features clave
- Offline-first (funciona sin red)
- Sincronización incremental
- Resolución de conflictos (last-write-wins → CRDTs)
- Versionado de documentos

---

## 📁 Estructura del proyecto

```
entropia/
  apps/
    desktop/          ← Tauri 2 shell
  packages/
    ui/               ← Design system (Svelte)
    store/            ← SQLite + Drizzle + state
    ai-pipeline/      ← AIProcessor interface + job queue + adapters
    ner/              ← NER adapter
    embeddings/       ← Embeddings + sqlite-vec search
    sync/             ← Sync layer (PocketBase o Cloudflare)
  services/
    workers/          ← Cloudflare Workers (si se usa CF)
```

---

## 🧩 Roadmap de desarrollo

### Fase 0 — Fundaciones
> **Goal**: infraestructura técnica que no bloquee ninguna fase posterior.

- [ ] Monorepo configurado (PNPM workspaces + Turborepo)
- [ ] Tauri 2 + Svelte con hot reload
- [ ] SQLite + Drizzle: schema base + migrations
- [ ] `packages/ui`: design system mínimo (tokens, componentes base)
- [ ] CI básico (lint + typecheck)

**Done when**: app desktop vacía corre, DB migra, CI verde.

---

### Fase 1 — MVP Documental
> **Goal**: validar el flujo de trabajo documental básico sin IA.

- [ ] Importación de imágenes y PDFs (drag & drop + file picker)
- [ ] Gestión de colecciones (CRUD)
- [ ] Visor de documentos (image/PDF)
- [ ] Metadata editable
- [ ] Notas por ítem
- [ ] Búsqueda por texto en metadata
- [ ] Exportación básica (JSON)

**Done when**: un historiador puede gestionar una colección de 100 documentos sin IA.

---

### Fase 2 — AI Pipeline Core
> **Goal**: infraestructura de IA que soporte todos los procesadores futuros.

- [ ] Interfaz `AIProcessor` implementada
- [ ] Job queue (pending / running / done / error)
- [ ] OCR adapter (olmOCR o Gemini)
- [ ] Procesamiento batch con progreso visible
- [ ] Almacenamiento de resultados en `ocr_results`
- [ ] Overlay de texto sobre imagen
- [ ] Layout analysis (bloques, columnas)

**Done when**: se pueden OCR-ear 50 documentos en batch y ver el resultado superpuesto.

---

### Fase 3 — Semántica
> **Goal**: extraer conocimiento estructurado del texto. Requiere Fase 2.

- [ ] NER (personas, lugares, fechas, organizaciones)
- [ ] Extracción de tripletes S-P-O
- [ ] Viewer de entidades por documento
- [ ] Embeddings por documento/chunk (sqlite-vec)
- [ ] Búsqueda semántica por similitud vectorial
- [ ] Vista de relaciones entre entidades

**Done when**: dado un corpus, se pueden encontrar documentos semánticamente relacionados y ver entidades extraídas.

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

🚧 En planificación — Fase 0
