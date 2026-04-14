1: # 🧠 EntropIA
2:
3: Análisis inteligente de fuentes históricas.
4:
5: ---
6:
7: ## 🎯 Descripción
8:
9: EntropIA es una aplicación desktop para el análisis de fuentes históricas digitalizadas mediante pipelines de inteligencia artificial.
10:
11: A diferencia de herramientas tradicionales de gestión documental, EntropIA no se limita a almacenar o visualizar documentos: produce capas de interpretación sobre materiales inherentemente fragmentarios, incompletos o degradados.
12:
13: El sistema combina:
14:
15: - Gestión documental local (offline-first)
16: - Procesamiento automático (OCR, NER, layout)
17: - Análisis semántico (tripletes, embeddings, grafos)
18: - Sincronización en la nube (self-hosted o edge) **planificada**
19:
20: ---
21:
22: ## 🚀 Diferenciales clave
23:
24: - OCR offline/local en desktop (✅ implementado)
25: - Named Entity Recognition (personas, lugares, fechas, instituciones) (✅ implementado)
26: - Embeddings + búsqueda semántica/FTS5 (✅ implementado)
27: - Extracción de tripletes semánticos (S-P-O) (✅ implementado, motor rule-based MVP)
28: - RAG sobre corpus documental (⏳ planificado)
29: - Exportación a herramientas externas:
30: - Tropy
31: - Recogito
32: - Gephi
33: - Voyant Tools
34:
35: ---
36:
37: ## 🏗️ Arquitectura
38:
39: `40: Desktop App (Tauri 2)
41: │
42: ├── UI (Svelte)
43: ├── Local DB (SQLite + Drizzle)
44: ├── File System (imágenes/PDFs)
45: │
46: ├── AI Pipeline (Adapters + Job Queue)
47: │   ├── OCR (engine.rs, commands.rs, preprocessor.rs, pdf.rs)
48: │   ├── NER (ner.rs)
49: │   ├── Embeddings (embeddings.rs, sqlite-vec)
50: │   └── Semantic Extraction (triples.rs, fts.rs)
51: │
52: └── Sync Layer (roadmap)
53:     ├── PocketBase (self-hosted, default)
54:     └── Cloudflare (D1 + R2 + Workers, escala)
55:`
56:
57: ---
58:
59: ## ⚙️ Stack tecnológico
60:
61: | Capa | Tecnología | Justificación |
62: | -------------- | --------------------------------- | -------------------------------------------- |
63: | Desktop | Tauri 2 | Ligero, acceso nativo FS |
64: | Frontend | Svelte | Mejor perf en desktop, bundle mínimo |
65: | DB local | SQLite | Offline-first |
66: | ORM | Drizzle | Compatible con SQLite + D1 |
67: | Vector search | sqlite-vec | Offline-first, sin dependencias externas |
68: | Job queue | In-process (SQLite) | Simple, offline-first, sin overhead |
69: | AI runtime | OCRS + fastembed + NLP rule-based | Offline-first, sin depender de APIs externas |
70: | Sync (roadmap) | PocketBase / Cloudflare (D1 + R2) | Objetivo de Fase 4 (aún no implementado) |
71:
72: ---
73:
74: ## 🤖 Pipeline de IA
75:
76: Arquitectura basada en adapters con job queue integrado:
77:
78: `ts
79: interface AIProcessor {
80:   process(input: DocumentInput): Promise<AIResult>
81: }
82: 
83: interface Job {
84:   id: string
85:   type: 'ocr' | 'ner' | 'embeddings' | 'triples'
86:   status: 'pending' | 'running' | 'done' | 'error'
87:   assetId: string
88:   createdAt: Date
89:   result?: AIResult
90:   error?: string
91: }
92: `
93:
94: ---
95:
96: ## 📊 Modelo de datos (simplificado)
97:
98: `sql
99: collections  (id TEXT PRIMARY KEY, name TEXT)
100: items        (id TEXT PRIMARY KEY, title TEXT, collection_id TEXT, created_at DATETIME)
101: assets       (id TEXT PRIMARY KEY, item_id TEXT, path TEXT, type TEXT)
102: notes        (id TEXT PRIMARY KEY, item_id TEXT, content TEXT)
103: 
104: -- AI outputs
105: extractions  (id TEXT PRIMARY KEY, asset_id TEXT, text_content TEXT, method TEXT, confidence REAL)
106: entities     (id TEXT PRIMARY KEY, item_id TEXT, entity_type TEXT, value TEXT, offsets, confidence)
107: triples      (id TEXT PRIMARY KEY, item_id TEXT, subject TEXT, predicate TEXT, object TEXT)
108: fts_items    (FTS5 virtual table: item_id, title, metadata, extracted_text)
109: vec_items    (sqlite-vec virtual table runtime, cuando extensión está disponible)
110: 
111: -- Pipeline
112: jobs         (id TEXT PRIMARY KEY, type TEXT, status TEXT, asset_id TEXT, created_at DATETIME, result JSON, error TEXT)
113: `
114:
115: ---
116:
117: ## 🔄 Cloud Sync (roadmap — aún no implementado)
118:
119: ### Opción 1 — PocketBase _(objetivo evaluado)_
120:
121: - Self-hosted simple
122: - Sincronización REST + realtime
123: - Sin infraestructura adicional
124: - **Estado**: no hay capa de sync activa en el código actual
125:
126: ### Opción 2 — Cloudflare _(objetivo de escala)_
127:
128: - D1 → metadata (SQLite serverless)
129: - R2 → almacenamiento de archivos
130: - Workers → API sync
131: - Durable Objects → colaboración / conflictos
132: - **Estado**: opción arquitectónica futura, no implementada en este repo
133:
134: ### Features clave
135:
136: - Offline-first (funciona sin red)
137: - Sincronización incremental (**planificada**)
138: - Resolución de conflictos (last-write-wins → CRDTs) (**planificada**)
139: - Versionado de documentos (**planificado**)
140:
141: ---
142:
143: ## 📁 Estructura del proyecto
144:
145: `146: entropia/
147:   apps/
148:     desktop/          ← Tauri 2 shell (16+ Rust files: db/nlp/ocr)
149:   packages/
150:     ui/               ← Design system (Svelte, 7 component tests)
151:     store/            ← SQLite + Drizzle + repos + migrations (12+ repo tests)
152:     config-ts/        ← tsconfig compartidos
153:   openspec/           ← specs, cambios y archivos SDD (vacío por ahora)
154: 
155:   # Nota: OCR, NER, embeddings y colas de trabajo en
156:   # apps/desktop/src-tauri/src/ (Rust backend).
157:`
158:
159: ---
160:
161: ## 📌 Estado actual
162:
163: > Última revisión del repo: **2026-04-13**
164:
165: - ✅ **Fase 0 completada** — fundaciones del monorepo, Tauri, SQLite/Drizzle, UI base, CI.
166: - ✅ **Fase 1 completada** — importación documental, CRUD, viewer, metadata, notas, búsqueda y export JSON.
167: - ✅ **Fase 2 completada** — OCR + job queue + persistencia + progreso visibles (overlay texto menor diferido).
168: - ✅ **Fase 3 completada** — NER + embeddings + FTS5 + tripletes S-P-O + viewer de entidades (relaciones graficas menor diferido).
169: - ⏳ **Fase 4 pendiente** — sincronización.
170: - ⏳ **Fase 5 pendiente** — knowledge graph / RAG / visualizaciones avanzadas.
171:
172: ### Tests & Calidad
173: - 26+ TS tests (UI/views/lib/repos): 67/67 green en ui (vitest).
174: - Rust backend: db/nlp/ocr modules con tests implícitos (CI verde).
175: - CI: lint/typecheck/test/build verde (salvo pnpm store mismatch — `pnpm install` fixes).
176:
177: ### Diferidos conocidos
178:
179: - Overlay texto OCR en visor.
180: - Vista relaciones/grafo entidades.
181: - Fallback PDFs escaneados + panel texto completo.
182:
183: ### Fuente de verdad del avance
184:
185: Este FLUJO.md resume el estado. No openspec/changes/archive/ aún (manual tracking).
186:
187: ### Implementado hoy (alto nivel)
188:
189: - Desktop app Tauri 2 + Svelte 5
190: - SQLite + Drizzle + migrations + repos tipados (asset/collection/item/note/job/extraction/entity/fts/embedding/triple)
191: - CRUD colecciones/items/assets, views (Collections/Collection/Item), TopBar, navigation
192: - Import/export, keyboard shortcuts
193: - OCR full pipeline (preprocessor/pdf/engine/commands)
194: - NLP: FTS5, NER, embeddings, triples S-P-O extraction + viewers
195: - Capabilities JSON system
196: - CI GitHub Actions full (TS/Rust/Windows)
197:
198: ---
199:
200: ## 🧩 Roadmap de desarrollo
201: _(Actualizado con progreso)_
202:
203: ### Fase 0 — Fundaciones ✅
204: _(completada)_
205:
206: ### Fase 1 — MVP Documental ✅
207: _(completada)_
208:
209: ### Fase 2 — AI Pipeline Core ✅
210: - ✅ Job queue + OCR adapter (Rust: engine/preprocessor/pdf)
211: - ✅ Persistencia resultados + progreso UI
212: _(Overlay texto → diferido menor)_
213:
214: ### Fase 3 — Semántica ✅
215: - ✅ NER + triples S-P-O + embeddings + FTS5 + entity viewer
216: _(Vista relaciones → diferido menor)_
217:
218: ### Fase 4 — Sync ⏳ PRÓXIMO
219: - Schema sync, incremental metadata/assets, conflictos
220:
221: ### Fase 5 — Knowledge Graph + Avanzado ⏳
222: - Grafo interactivo, RAG chat, timelines, maps, exports
223:
224: ---
225:
226: ## 📌 Estado
227:
228: 🚀 **Fases 0-3 listas** — MVP funcional con IA local. Listo para multi-dispositivo.
229:
230: ### Próximo paso recomendado
231:
232: - **Fase 4 Sync**: PocketBase self-hosted (parallelizable).
233: - Close diferidos Fase2/3 (overlay/grapho).
234: - `pnpm install` para fix tests desktop/store.
