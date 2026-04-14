# EntropIA 🧠

## Herramienta Académica para Análisis Computacional en Humanidades Digitales

**EntropIA** es una aplicación de escritorio open-source diseñada para investigadores en **ciencias sociales y humanidades**, con énfasis en **historia digital** y prácticas de archivo computacional. Orientada al trabajo con **fuentes primarias digitalizadas** (imágenes, PDFs escaneados, documentos fragmentarios), EntropIA media la **construcción de corpus, interpretación automatizada y producción de conocimiento estructurado**.

En el contexto de la investigación cualitativa a escala, donde las fuentes son inherentemente incompletas o degradadas, EntropIA genera **capas interpretativas** locales y offline:

- **Del caos al corpus estructurado**: Importación, organización y metadata enriquecida.
- **Mediación computacional de la lectura**: OCR + NER para acceso textual.
- **Análisis semántico exploratorio**: Triples S-P-O, embeddings, búsqueda híbrida.
- **Hacia el conocimiento accionable**: Pipeline hacia grafos, RAG y exports DH.

**Posicionamiento único**:

- **Offline-first & Privado**: SQLite local, zero telemetry — ideal para archivos sensibles.
- **Escalable a corpus grandes**: Job queues, FTS5 + vectores.
- **Integrado con ecosistema DH**: Exports compatibles con Tropy, Recogito, Gephi, Voyant Tools.
- **Extensible por investigadores**: Capabilities JSON para workflows custom (ej: NER domain-specific).

## Contexto de Investigación & Problema Resuelto

Las humanidades digitales (DH) transforman la investigación mediante cómputo, pero herramientas existentes se limitan:

- **Gestión** (Zotero/Omeka): Metadata manual, sin IA raw sources.
- **OCR/Transcripción** (Transkribus): Cloud-dependiente, costoso para no-anglés.
- **Análisis** (Voyant/Mallet): Requiere pre-procesado externo.

**EntropIA cierra el loop**: De fuente raw → corpus anotado → insights semánticos, **todo local**. Facilita:

- **Construcción de corpus reproducibles**.
- **Análisis hermenéutico asistido** (entidades/relaciones automáticas).
- **Colaboración edge** (sync roadmap).

Ver [FLUJO.md](FLUJO.md) para roadmap alineado con prácticas DH.

## ✨ Capacidades Clave (framed for research)

| Pipeline            | Funcionalidad                            | Output para Investigación                                |
| ------------------- | ---------------------------------------- | -------------------------------------------------------- |
| **OCR**             | Texto de imágenes/PDFs degradados        | Transcripciones base para corpus                         |
| **NER**             | Entidades (personas/lugares/fechas/orgs) | Índice onomástico automático                             |
| **Semántica**       | Triples S-P-O rule-based                 | Base para knowledge graphs                               |
| **Embeddings/FTS5** | Búsqueda híbrida semántica               | Queries contextuales (\"conflictos 1930s Buenos Aires\") |
| **Jobs Queue**      | Procesamiento batch con progreso         | Escalabilidad a 1000s docs                               |
| **Viewers**         | Entidades/triples por doc                | Exploración cualitativa asistida                         |

## 🛠️ Stack Técnico

| Capa         | Tecnología                   | Razón Académica                              |
| ------------ | ---------------------------- | -------------------------------------------- |
| **Desktop**  | Tauri 2 (Rust/WebView)       | Nativo FS, ligero para laptops investigación |
| **Frontend** | Svelte 5                     | Reactivo, bajo bundle para corpus grandes    |
| **DB**       | SQLite + Drizzle             | Portable, FTS5/vec offline                   |
| **AI Local** | Rust crates (OCRS/fastembed) | Sin APIs, reproducible                       |
| **Estado**   | TS Repos (Drizzle) + Tests   | Typed safety para datos sensibles            |

**Tests**: 26+ TS (67/67 green UI/repos), Rust coverage sólido.

## 🚀 Instalación & Uso Rápido

```bash
git clone https://github.com/agusnieto77/EntropIA.git
cd EntropIA
pnpm install  # Fixea store mismatch si hay warnings

# Dev (hot reload)
pnpm run tauri dev

# Build release
pnpm run tauri build
```

**Primeros pasos investigación**:

1. Importa corpus (drag/drop carpetas docs).
2. Ejecuta OCR/NER batch.
3. Explora triples/entidades en views.
4. Query semántica → refine corpus.

## 📊 Estado del Proyecto

**Alpha estable (Fases 0-3 completas)** — MVP funcional para corpus ~1000 docs.

- ✅ Backend repos (assets/collections/items/jobs/entities/etc.) + tests.
- ✅ UI Views (Collections/Item/Entity), Navigation/TopBar.
- ✅ Pipeline completo OCR/NLP/FTS.
- 🔄 Capabilities engine.
- ⏳ Sync multi-dispositivo (Fase 4 próximo), KG/RAG (Fase 5).

Detalle en [FLUJO.md](FLUJO.md).

## 🤝 Contribución Académica

Ideal para colaboradores DH/historia:

1. [Issue](https://github.com/agusnieto77/EntropIA/issues/new) con contexto investigación.
2. Branch `feat/domain-ner-historia-argentina`.
3. Commits convencionales + tests.
4. PRs bienvenidas: NER custom, exports Zotero, etc.

**Cita**: Nieto, A. (2026). EntropIA: Local AI Pipeline for Digital History Sources. GitHub: agusnieto77/EntropIA.

---

**Powered by local compute. Fuentes sensibles → conocimiento abierto. Para humanidades que escalan.**

⭐ Si usás en papers/projects, citá!
