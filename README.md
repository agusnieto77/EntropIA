# EntropIA 🧠

> ⚠️ **Beta activa.** EntropIA está en desarrollo y el modelo de datos, la UX y algunos pipelines todavía pueden cambiar.

## Herramienta de escritorio para corpus, OCR y análisis asistido

Desarrollado por [**HLab (Laboratorio de Humanidades Digitales)**](https://hlab.com.ar/).

**EntropIA** es una app de escritorio open-source orientada a investigación en humanidades y ciencias sociales. Está pensada para trabajar con **imágenes, PDFs y audio** de forma **local/offline-first**, construir corpus y sumar capas de análisis asistido sobre las fuentes.

**Release actual:** [`v0.0.9`](https://github.com/agusnieto77/EntropIA/releases/tag/v0.0.9)

> Si querés probar la app sin compilar, andá directo a [GitHub Releases](https://github.com/agusnieto77/EntropIA/releases).

Hoy el foco del proyecto está en:

- **organización de corpus** en colecciones e ítems
- **OCR** para imágenes y PDFs
- **transcripción** de audio
- **extracción de entidades** y **triples**
- **FTS + embeddings** para búsqueda y similitud
- **anotaciones, notas y edición manual** sobre los resultados

## Qué ofrece hoy

### Ingesta y organización

- colecciones, ítems y assets locales
- soporte para **imágenes, PDFs y audio**
- persistencia local en **SQLite**

### OCR y transcripción

- **OCR Light (OCRL)**: OCR plano para imágenes y PDFs
- **OCR High (OCRH)**: modo con **PaddleOCR-VL** para extracción más rica y sensible al layout
- extracción de texto nativo desde PDF cuando la calidad lo permite
- transcripción de audio con **faster-whisper** vía subprocess de Python
- edición manual de OCR/transcripción con re-enriquecimiento posterior

### NLP y exploración

- **NER** con pipeline híbrido (**ONNX + spaCy** cuando está disponible)
- extracción de **triples S-P-O**
- indexación **FTS**
- **embeddings** y búsqueda de ítems similares
- **topics** editables y sugerencias reutilizables
- visualización geográfica de entidades resueltas en mapa
- procesamiento en **background jobs** con eventos de progreso

### Trabajo sobre documentos

- visor de documento
- panel de entidades y triples por ítem y/o asset según el contexto activo
- **anotaciones** sobre assets
- **notas** asociadas al documento/asset activo
- edición de metadata

## Modelos usados hoy (por proceso)

| Proceso | Modelo / runtime actual |
| --- | --- |
| LLM local (OCR correction, summaries, triples, tareas asistidas) | **`gemma-4-E2B-it-Q4_K_M.gguf`** vía `llama.cpp` (`llama-cpp-2`), `n_ctx=4096` |
| Transcripción de audio | **`faster-whisper/base`** vía subprocess de Python (`compute_type=int8`, idioma por defecto `es`) |
| Embeddings | **`sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2`** vía `fastembed` en Python |
| NER local principal | modelo **ONNX local** en `resources/models/ner/model.onnx` |
| NER opcional / fallback enriquecido | **`es_core_news_lg`** vía spaCy en Python |
| OCR High (OCRH) | **PaddleOCR-VL** vía `paddleocr[doc-parser]` en Python |
| OCR nativo (cuando `paddle-ocr` está habilitado) | **`PP-OCRv5_mobile_det.mnn`** + **`latin_PP-OCRv5_mobile_rec_infer.mnn`** |
| Corrección de orientación OCR nativo | **`PP-LCNet_x1_0_doc_ori.mnn`** |
| Detección de layout ONNX (hoy no activa en producción) | **`PP-DocLayout-L.onnx`** |
| Fallback OCR clásico | **Tesseract** (`spa+eng`) |

> Nota: varios pipelines tienen **degradación elegante**. Si falta un runtime, modelo o dependencia opcional, EntropIA intenta seguir funcionando con el mejor fallback disponible.

## Botones cableados hoy: UI → función → comando → backend

> En esta tabla, **LLMCloud** significa el proveedor remoto de inferencia LLM configurado en la app. Hoy puede ser OpenRouter, pero el provider puede cambiar.

> Referencia rápida de labels actuales en UI:
> - `OCRL` = OCR Light
> - `OCRH` = OCR High
> - `OCRC` = OCR Correction por LLM
> - `OCRR` = OCR/Transcription Summary por LLM
> - `TRIPLET` = extracción de triples semánticos por LLM

| Botón visible | Dónde aparece | Archivo frontend | Función frontend | Comando Tauri | Archivo backend | Backend efectivo |
| --- | --- | --- | --- | --- | --- | --- |
| `OCRL` | Vista de ítem, sección OCR | `apps/desktop/src/views/ItemView.svelte` | `handleExtractText(selectedAsset, 'light')` | `extract_text` | `apps/desktop/src-tauri/src/ocr/commands.rs` | **Local** (PaddleOCR / Tesseract según disponibilidad) |
| `OCRH` | Vista de ítem, sección OCR | `apps/desktop/src/views/ItemView.svelte` | `handleExtractText(selectedAsset, 'high')` | `extract_text` | `apps/desktop/src-tauri/src/ocr/commands.rs` | **Local** (PaddleOCR-VL por Python; con fallback local si corresponde) |
| `OCRC` | Vista de ítem, sección OCR | `apps/desktop/src/views/ItemView.svelte` | `handleLlmCorrectOcr()` | `llm_correct_ocr` / `llm_correct_ocr_asset` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` |
| `OCRR` | Vista de ítem, sección OCR | `apps/desktop/src/views/ItemView.svelte` | `handleLlmSummarize()` | `llm_summarize` / `llm_summarize_asset` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` |
| `Transcribe` | Vista de ítem, sección audio | `apps/desktop/src/views/ItemView.svelte` | `handleTranscribeAudio(selectedAsset)` | `transcribe_audio` | `apps/desktop/src-tauri/src/transcription/commands.rs` | **Local** (Python + `faster-whisper`) |
| `OCRR` | Vista de ítem, sección audio | `apps/desktop/src/views/ItemView.svelte` | `handleLlmSummarize()` | `llm_summarize` / `llm_summarize_asset` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` |
| `INDEX` | Vista de ítem, sección NLP | `apps/desktop/src/views/ItemView.svelte` | `handleIndexFts()` | `index_fts` | `apps/desktop/src-tauri/src/nlp/commands.rs` | **Local** (SQLite FTS) |
| `EMBED` | Vista de ítem, sección NLP | `apps/desktop/src/views/ItemView.svelte` | `handleEmbedItem()` | `embed_item` | `apps/desktop/src-tauri/src/nlp/commands.rs` | **Local** (Python + `fastembed`) |
| `NER` | Vista de ítem, sección NLP | `apps/desktop/src/views/ItemView.svelte` | `handleExtractEntities()` | `extract_entities` / `extract_entities_for_asset` | `apps/desktop/src-tauri/src/nlp/commands.rs` | **Local** (ONNX + spaCy opcional) |
| `TRIPLET` | Vista de ítem, sección NLP | `apps/desktop/src/views/ItemView.svelte` | `handleLlmExtractTriples()` | `llm_extract_triples` / `llm_extract_triples_asset` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` |

### Capacidades implementadas pero no visibles todavía en la UI

| Capacidad | Wrapper frontend | Archivo frontend | Comando Tauri | Archivo backend | Backend efectivo | Estado UI |
| --- | --- | --- | --- | --- | --- | --- |
| Q&A sobre colección | `llmAsk(collectionId, question)` | `apps/desktop/src/lib/llm.ts` | `llm_ask` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` | **No cableado** |
| Extracción de entidades por LLM | `llmExtractEntities(itemId)` / `llmExtractEntitiesAsset(assetId)` | `apps/desktop/src/lib/llm.ts` | `llm_extract_entities` / `llm_extract_entities_asset` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` | **No cableado** |
| Clasificación por LLM | `llmClassify(itemId, categories)` | `apps/desktop/src/lib/llm.ts` | `llm_classify` | `apps/desktop/src-tauri/src/llm/commands.rs` | **LLM local o LLMCloud**, según `llm_mode` | **No cableado** |

## Stack técnico real

| Capa | Tecnología |
| --- | --- |
| Desktop | **Tauri 2** + Rust |
| Frontend | **Svelte 5** + Vite |
| DB local | **SQLite** |
| Store/UI | workspace packages (`@entropia/store`, `@entropia/ui`) |
| ORM cliente | **Drizzle ORM** |
| OCR | **Tesseract**, `ocr-rs` (feature `paddle-ocr`), **PaddleOCR-VL** vía Python |
| Transcripción | **faster-whisper** vía Python |
| Embeddings | **fastembed** vía Python |
| NER | ONNX local + **spaCy** opcional |
| LLM local | **llama.cpp** + GGUF (**Gemma 4 E2B IT Q4_K_M**) |

## Instalación

### Descarga directa

Podés bajar instaladores desde [GitHub Releases](https://github.com/agusnieto77/EntropIA/releases).

| Sistema operativo | Instalador |
| --- | --- |
| Windows 10/11 (x64) | `.msi` o `.exe` |
| macOS Apple Silicon | `.dmg` |
| macOS Intel | `.dmg` |
| Linux | `.deb` o `.AppImage` |

> En macOS, si aparece el warning de “desarrollador no identificado”, abrí con clic derecho → **Abrir**.

### Qué necesitás para la experiencia completa

La app puede abrirse y usarse sin tener todo el stack local instalado, pero algunas capacidades avanzadas dependen de runtimes externos.

| Capacidad | Requiere |
| --- | --- |
| OCR básico / fallback | Tesseract disponible |
| OCR High (OCRH) | Python + `paddleocr[doc-parser]` |
| Transcripción | Python + `faster-whisper` |
| Embeddings | Python + `fastembed` |
| NER enriquecido opcional | Python + `spacy` + `es_core_news_lg` |

> En la práctica, **Windows es hoy la plataforma mejor documentada y más verificada** para levantar el stack completo de OCR/NLP local.

## Desarrollo desde código fuente

### Requisitos generales

- **Node.js 22+**
- **pnpm 9**
- **Rust** (toolchain estable)

Dependencias base por sistema:

- **Linux (Ubuntu/Debian)**: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`
- **macOS**: Xcode Command Line Tools (`xcode-select --install`)
- **Windows**: WebView2 + toolchain MSVC

### Dependencias del frontend/desktop

```bash
git clone https://github.com/agusnieto77/EntropIA.git
cd EntropIA
pnpm install
```

### Ejecutar en desarrollo

```bash
pnpm --filter @entropia/desktop tauri dev
```

### Validar sólo la app desktop

```bash
pnpm --filter @entropia/desktop test
pnpm --filter @entropia/desktop lint
pnpm --filter @entropia/desktop typecheck
```

### Build

```bash
pnpm --filter @entropia/desktop tauri build
```

## Requisitos adicionales para OCR/NLP local

La app puede **degradar con gracia** si faltan dependencias opcionales, pero para tener el stack completo de OCR, transcripción y NLP local necesitás herramientas adicionales.

### Windows

#### Toolchain nativo

- Visual Studio Build Tools 2022 (MSVC)
- LLVM/Clang para `bindgen`
- Tesseract instalado vía `vcpkg`

#### Tesseract (vcpkg)

```powershell
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install tesseract:x64-windows-static-md
C:\vcpkg\vcpkg integrate install
```

#### LLVM

```powershell
choco install llvm -y
```

Variable recomendada:

```powershell
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

#### Python y paquetes

Necesitás **Python 3.8+** con estos paquetes:

- `faster-whisper`
- `fastembed`
- `paddleocr[doc-parser]`
- `spacy`
- `es_core_news_lg`

Ejemplo:

```powershell
pip install faster-whisper fastembed "paddleocr[doc-parser]" spacy
python -m spacy download es_core_news_lg
```

#### Variables útiles

```powershell
[System.Environment]::SetEnvironmentVariable("TESSDATA_PREFIX", "C:\vcpkg\installed\x64-windows-static-md\share", "User")
```

> Recomendación práctica: si vas a desarrollar o validar OCR/NLP local de punta a punta, arrancá por **Windows**. Es el entorno con mejor cobertura documental y el más probado en este repo hoy.

## Scripts útiles del monorepo

Desde la raíz:

```bash
pnpm dev
pnpm build
pnpm lint
pnpm typecheck
pnpm test
```

## Estado del proyecto

**Beta funcional**. Hoy ya existe un flujo usable para:

- importar fuentes
- correr OCR / transcripción
- enriquecer con NER / triples / FTS / embeddings
- navegar documentos y resultados
- editar texto extraído, metadata, notas y anotaciones

Todavía hay trabajo abierto en estabilidad, DX, roadmap de sync/export y refinamiento de algunos pipelines.

## Notas

- EntropIA privilegia **procesamiento local** y puede degradar algunas capacidades si faltan dependencias opcionales de Python o del toolchain nativo.
- El stack de OCR/NLP más completo hoy está **mejor documentado y más verificado en Windows**.
- Los instaladores publicados sirven para probar la app rápido; el stack local completo requiere dependencias adicionales si querés OCR/NLP avanzado.
- El roadmap sigue abierto para estabilidad, sync/export y refinamiento de pipelines.

---

**Powered by local compute.**
