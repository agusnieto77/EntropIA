# EntropIA 🧠

> ⚠️ **Software en desarrollo — versión beta.** EntropIA es una herramienta activa de investigación. Funcionalidades clave están operativas, pero la API, el modelo de datos y la interfaz pueden cambiar sin aviso previo. No se recomienda su uso en entornos de producción sin validación previa.

## Herramienta para análisis computacional en Humanidades Digitales

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

## Contexto de investigación

- **Construcción de corpus reproducibles**.
- **Análisis hermenéutico asistido** (entidades/relaciones automáticas).
- **Colaboración edge** (sync roadmap).

## ✨ Capacidades Clave (framed for research)

| Pipeline            | Funcionalidad                            | Output                                                 |
| ------------------- | ---------------------------------------- | ------------------------------------------------------ |
| **OCR**             | Texto de imágenes/PDFs degradados        | Transcripciones base para corpus                       |
| **NER**             | Entidades (personas/lugares/fechas/orgs) | Índice onomástico automático                           |
| **Semántica**       | Triples S-P-O rule-based                 | Base para knowledge graphs                             |
| **Embeddings/FTS5** | Búsqueda híbrida semántica               | Queries contextuales ("conflictos 1930s Buenos Aires") |
| **Jobs Queue**      | Procesamiento batch con progreso         | Escalabilidad a 1000s docs                             |
| **Viewers**         | Entidades/triples por doc                | Exploración cualitativa asistida                       |

## 🛠️ Stack Técnico

| Capa         | Tecnología                   | Razón                                        |
| ------------ | ---------------------------- | -------------------------------------------- |
| **Desktop**  | Tauri 2 (Rust/WebView)       | Nativo FS, ligero para laptops investigación |
| **Frontend** | Svelte 5                     | Reactivo, bajo bundle para corpus grandes    |
| **DB**       | SQLite + Drizzle             | Portable, FTS5/vec offline                   |
| **AI Local** | Rust crates (OCRS/fastembed) | Sin APIs, reproducible                       |
| **Estado**   | TS Repos (Drizzle) + Tests   | Typed safety para datos sensibles            |

## 🚀 Instalación

### Descarga directa (recomendado)

Descargá el instalador para tu sistema desde [GitHub Releases](https://github.com/agusnieto77/EntropIA/releases).

| Sistema operativo                     | Instalador                         | Instrucciones                                                                                                                                          |
| ------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Windows** 10/11 (x64)               | `EntropIA_x64_en-US.msi` o `.exe`  | Descargá y ejecutá. El instalador guía el proceso.                                                                                                     |
| **macOS** Apple Silicon (M1/M2/M3/M4) | `EntropIA_aarch64.dmg`             | Abrí el `.dmg`, arrastrá EntropIA a Aplicaciones. En la primera apertura: clic derecho → Abrir (necesario porque no está firmado con Apple Developer). |
| **macOS** Intel                       | `EntropIA_x64.dmg`                 | Mismo proceso que Apple Silicon.                                                                                                                       |
| **Linux** (Ubuntu/Debian)             | `EntropIA_amd64.deb` o `.AppImage` | **.deb**: `sudo dpkg -i entropia_*.deb` · **.AppImage**: `chmod +x EntropIA_*.AppImage && ./EntropIA_*.AppImage`                                       |

> ⚠️ macOS: al abrir por primera vez vas a ver un warning de "desarrollador no identificado". Clic derecho → Abrir → Abrir de todos modos.

### Instalación desde código fuente (desarrollo)

<details>
<summary><strong>Prerrequisitos</strong></summary>

Todos los sistemas necesitan:

- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 9.x (`npm install -g pnpm@9`)
- [Rust](https://rustup.rs/) toolchain 1.88+

Dependencias adicionales por sistema:

**Linux** (Ubuntu/Debian):

```bash
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

**Windows**: WebView2 ya viene incluido en Windows 10 (1903+) y Windows 11.

**macOS**: Xcode Command Line Tools (`xcode-select --install`). WebKit es nativo del sistema.

</details>

<details>
<summary><strong>Clonar, instalar y ejecutar</strong></summary>

```bash
git clone https://github.com/agusnieto77/EntropIA.git
cd EntropIA
pnpm install

# Dev (hot reload)
pnpm --filter @entropia/desktop tauri dev

# Build release (genera instalador en src-tauri/target/release/bundle/)
pnpm --filter @entropia/desktop tauri build
```

</details>

### Primeros pasos investigación

1. Importa corpus (drag/drop carpetas docs).
2. Ejecuta OCR/NER batch.
3. Explora triples/entidades en views.
4. Query semántica → refine corpus.

## 📊 Estado del Proyecto

**Beta en desarrollo (Fases 0-3 completas)** — MVP funcional para corpus ~1000 docs. La aplicación es usable pero aún no se considera estable para producción.

- ✅ Backend repos (assets/collections/items/jobs/entities/etc.) + tests.
- ✅ UI Views (Collections/Item/Entity), Navigation/TopBar.
- ✅ Pipeline completo OCR/NLP/FTS.
- 🔄 Capabilities engine.
- ⏳ Sync multi-dispositivo (Fase 4 próximo), KG/RAG (Fase 5).

---

**Powered by local compute.**
