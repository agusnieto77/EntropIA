# EntropIA 🧠

> ⚠️ **Beta activa.** EntropIA está en desarrollo y el modelo de datos, la UX y algunos pipelines todavía pueden cambiar.

## Herramienta de escritorio para corpus, OCR y análisis asistido

Desarrollado por [**HLab (Laboratorio de Humanidades Digitales)**](https://hlab.com.ar/).

**EntropIA** es una app de escritorio open-source orientada a investigación en humanidades y ciencias sociales. Está pensada para trabajar con **imágenes, PDFs y audio** de forma **local/offline-first**, construir corpus y sumar capas de análisis asistido sobre las fuentes.

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

- **OCR Light (OCRL)**: OCR plano para imágenes/PDFs
- **OCR High (OCRH)**: modo con **PaddleOCR-VL** para extracción más rica y sensible al layout
- extracción de texto nativo desde PDF cuando la calidad lo permite
- transcripción de audio con **faster-whisper** vía subprocess de Python
- edición manual de OCR/transcripción con re-enriquecimiento posterior

### NLP y exploración

- **NER** con pipeline híbrido (**ONNX + spaCy** cuando está disponible)
- extracción de **triples S-P-O**
- indexación **FTS**
- **embeddings** y búsqueda de ítems similares
- procesamiento en **background jobs** con eventos de progreso

### Trabajo sobre documentos

- visor de documento
- panel de entidades y triples por ítem
- **anotaciones** sobre assets
- **notas** por ítem
- edición de metadata

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

### Build

```bash
pnpm --filter @entropia/desktop tauri build
```

## Requisitos adicionales para OCR/NLP local

La app puede **degradar con gracia** si faltan dependencias opcionales, pero para tener el stack completo de OCR/transcripción/NLP local necesitás herramientas adicionales.

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
- El stack de OCR/NLP más completo hoy está mejor documentado y verificado en **Windows**.
- El roadmap sigue abierto para estabilidad, sync/export y refinamiento de pipelines.

---

**Powered by local compute.**
