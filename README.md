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
  - Recogito
  - Gephi
  - Voyant Tools

---

## 🏗️ Arquitectura

```
Desktop App (Tauri)
│
├── UI (React/Svelte)
├── Local DB (SQLite + Drizzle)
├── File System (imágenes/PDFs)
│
├── AI Pipeline (Adapters)
│   ├── OCR
│   ├── NER
│   ├── Embeddings
│   └── Semantic Extraction
│
└── Sync Layer
    ├── Cloudflare (D1 + R2 + Workers)
    └── / or Self-hosted (PocketBase / Supabase)
```

---

## ⚙️ Stack tecnológico

| Capa            | Tecnología                | Justificación |
|-----------------|--------------------------|--------------|
| Desktop         | Tauri 2                  | Ligero, acceso nativo FS |
| Frontend        | React / Svelte           | Ecosistema maduro |
| DB local        | SQLite                   | Offline-first |
| ORM             | Drizzle                  | Compatible con SQLite + D1 |
| AI APIs         | DeepInfra / Gemini       | OCR + NLP |
| Sync Cloud      | Cloudflare (D1 + R2)     | Edge + SQLite-compatible |
| Alternativa     | PocketBase / Supabase    | Self-hosting |

---

## 🤖 Pipeline de IA

Arquitectura basada en adapters:

```ts
interface AIProcessor {
  process(input: DocumentInput): Promise<AIResult>
}
```

---

## 📊 Modelo de datos (simplificado)

```sql
items (id TEXT PRIMARY KEY, title TEXT, collection_id TEXT, created_at DATETIME)
assets (id TEXT PRIMARY KEY, item_id TEXT, path TEXT, type TEXT)
ocr_results (id TEXT PRIMARY KEY, asset_id TEXT, text TEXT, layout JSON)
entities (id TEXT PRIMARY KEY, name TEXT, type TEXT)
relations (id TEXT PRIMARY KEY, subject_id TEXT, predicate TEXT, object_id TEXT)
embeddings (id TEXT PRIMARY KEY, ref_id TEXT, vector BLOB)
collections (id TEXT PRIMARY KEY, name TEXT)
notes (id TEXT PRIMARY KEY, item_id TEXT, content TEXT)
```

---

## 🔄 Cloud Sync

### Opción 1 — Cloudflare
- D1 → metadata (SQLite serverless)
- R2 → almacenamiento de archivos
- Workers → API sync
- Durable Objects → colaboración / conflictos

### Opción 2 — Self-hosted
- PocketBase
- Supabase

### Features clave
- Offline-first
- Sincronización incremental
- Resolución de conflictos (CRDTs)
- Versionado de documentos

---

## 🧩 Roadmap de desarrollo

### Fase 1 — MVP
- Importación de imágenes/PDFs
- Gestión de colecciones
- Visor de documentos
- Metadata editable
- Búsqueda básica

### Fase 2 — OCR + Layout
- Integración OCR
- Procesamiento batch
- Visualización overlay

### Fase 3 — Semántica
- NER
- Tripletes
- Embeddings
- Búsqueda semántica

### Fase 4 — Sync
- Sincronización
- Multi-dispositivo

### Fase 5 — Avanzado
- RAG
- Grafos
- Mapas
- Timeline

---

## 📁 Estructura del proyecto

```
entropia/
  apps/
    desktop/
  packages/
    engine/
    store/
    ui/
    sync/
  services/
    workers/
```

---

## 📌 Estado

🚧 En planificación
