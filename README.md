# EntropIA 🌌

**Domá tu caos digital con IA local: organiza fotos, docs, notas y media en colecciones inteligentes.**

EntropIA es una **app de escritorio** (Tauri: Rust + Svelte) para gestionar tu **entropía personal**:

- **NLP Semántico**: Búsquedas naturales, extracción de triples (sujeto-predicado-objeto).
- **OCR Inteligente**: Texto de imágenes/PDFs.
- **Colecciones Dinámicas**: Assets/Items agrupados por reglas/capabilidades.
- **100% Offline**: SQLite con FTS5, sin nubes.
- **Extensible**: Capabilities en JSON para workflows custom.

## ✨ Features

- 🔍 **Búsqueda Avanzada**: FTS + semántica para queries como \"fotos de la playa con amigos\".
- 📸 **OCR Automático**: Extrae texto y metadata de visuals.
- 🗂️ **Repositorios Tipados**: Assets, Collections, Items con tests.
- ⚡ **Rápido & Ligero**: Rust backend, Svelte UI reactiva.
- 🔧 **Capabilities**: Define acciones dinámicas (ej: taggear, mover, procesar).

## 🛠️ Tech Stack

| Capa         | Tech                                    |
| ------------ | --------------------------------------- |
| **Frontend** | Svelte 5 + Tauri                        |
| **Backend**  | Rust + SQLite (FTS5 NLP)                |
| **Estado**   | Custom Store (TS repos + tests)         |
| **OCR/NLP**  | Rust crates + custom parsers            |
| **UI**       | Svelte components (Views, TopBar, etc.) |

## 🚀 Quick Start

```bash
# Clona e instala
git clone https://github.com/agusnieto77/EntropIA.git
cd EntropIA
npm install

# Dev mode
npm run tauri dev

# Build
npm run tauri build
```

## 📊 Status

**Alpha v0.1** – Fase 3/5 ([FLUJO.md](FLUJO.md) para roadmap detallado).

- ✅ Backend repos + tests
- ✅ UI Views + Navigation
- ✅ NLP FTS + OCR basics
- 🔄 Capabilities engine
- ⏳ Import masivo + Sync

## 🤝 Contribuye

1. Forkea y crea issue ([template](https://github.com/agusnieto77/EntropIA/issues/new)).
2. Branch: `feat/tu-feature`
3. Commit convencional, PR con tests.

**¡Gracias por chequear!** Star ⭐ si te copa el vibe. Questions? @agusnieto77.

---

_Powered by local AI, zero telemetry. Tu data, tu máquina._
