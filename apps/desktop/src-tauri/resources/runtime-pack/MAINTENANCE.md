# Runtime-pack maintenance contract

## Qué cubre este repo

- Manifiestos versionados por plataforma.
- Estructura bundleable para `windows-x86_64` y `linux-x86_64`.
- Scripts de assembly y smoke (`scripts/build_runtime_pack.py`, `scripts/runtime-pack-smoke.py`).
- Fixtures chicos para validar wiring sin subir payloads pesados al repo.

## Qué entra por release-time artifact injection

Antes de publicar una release que diga “self-contained”, CI/release DEBE reemplazar los fixtures por:

1. Python relocatable redistribuible por plataforma.
2. `uv` auditado si cambia respecto del fixture.
3. Wheelhouse offline real para OCR/transcripción/NLP.
4. Caches/modelos presembrados (HF, PaddleX, spaCy) requeridos por los flujos core.
5. Shared libraries Linux auditadas (`libpdfium.so`, `libonnxruntime.so`, y cualquier dependencia adicional que resulte obligatoria).

## Contrato de payload externo

- El script `scripts/build_runtime_pack.py` ahora acepta `--payload-root`.
- Ese directorio puede venir como layout directo (`python/`, `uv/`, `wheelhouse/`, `caches/`, `resources/lib/`) o como `<payload-root>/<platform>/...`.
- Si existe `manifest.overrides.json`, el script aplica esos overrides al manifest final y **recalcula** los listados/checksums/tamaños a partir de los archivos realmente ensamblados.
- Workflow de release: busca payloads externos en `${RUNNER_TEMP}/runtime-payloads/`. Si no aparecen, arma fixture packs de forma explícita.
- El workflow ensambla primero en `apps/desktop/src-tauri/target/runtime-pack/`, corre smoke ahí y recién después reemplaza `resources/runtime-pack/<platform>` para que el `tauri-action` bundlee el payload real sin destruir la fuente fixture durante el armado.

### Layouts aceptados para `--payload-root`

Layout directo:

```text
runtime-payloads/
├── manifest.overrides.json
├── python/
├── uv/
├── wheelhouse/
├── caches/
└── resources/lib/
```

Layout por plataforma:

```text
runtime-payloads/
├── windows-x86_64/
│   ├── manifest.overrides.json
│   ├── python/
│   ├── uv/
│   ├── wheelhouse/
│   ├── caches/
│   └── resources/lib/
└── linux-x86_64/
    ├── manifest.overrides.json
    ├── python/
    ├── uv/
    ├── wheelhouse/
    ├── caches/
    └── resources/lib/
```

### Handoff real por plataforma

| Plataforma | `python_relpath` esperado | `uv_relpath` esperado | Native assets mínimos | Artifactos externos mínimos |
| ---------- | ------------------------- | --------------------- | --------------------- | --------------------------- |
| `windows-x86_64` | `python/python.exe` | `uv/uv.exe` | `resources/lib/pdfium.dll` | `relocatable-python-windows-x86_64`, `offline-wheelhouse-core`, `seeded-model-caches` |
| `linux-x86_64` | `python/bin/python3` | `uv/bin/uv` | `resources/lib/libpdfium.so`, `resources/lib/libonnxruntime.so` | `relocatable-python-linux-x86_64`, `offline-wheelhouse-core`, `seeded-model-caches`, `linux-native-libs` |

### Output verificable del armado

- Cada corrida de `build_runtime_pack.py` deja `target/runtime-pack/<platform>/assembly-summary.json` con el `payload_root` resuelto, el perfil final y el listado de archivos ensamblados.
- `runtime-pack-smoke.py` acepta como `--root` tanto el directorio padre (`target/runtime-pack/`) como el directorio puntual de plataforma (`target/runtime-pack/<platform>`).
- La validación útil para handoff real es: **armar con payload externo → revisar `assembly-summary.json` → correr smoke sobre ese output**.

Ejemplos de validación manual con payload real:

```bash
python3 apps/desktop/src-tauri/scripts/build_runtime_pack.py --platform windows-x86_64 --payload-root /abs/path/runtime-payloads --output-dir apps/desktop/src-tauri/target/runtime-pack
python3 apps/desktop/src-tauri/scripts/runtime-pack-smoke.py --platform windows-x86_64 --root apps/desktop/src-tauri/target/runtime-pack

python3 apps/desktop/src-tauri/scripts/build_runtime_pack.py --platform linux-x86_64 --payload-root /abs/path/runtime-payloads --output-dir apps/desktop/src-tauri/target/runtime-pack
python3 apps/desktop/src-tauri/scripts/runtime-pack-smoke.py --platform linux-x86_64 --root apps/desktop/src-tauri/target/runtime-pack
```

Ejemplo mínimo de `manifest.overrides.json` para una inyección completa:

```json
{
  "payload_profile": "release",
  "release_injection_required": false,
  "external_artifacts_required": []
}
```

## Regla de verdad

Si `payload_profile != release` o `release_injection_required = true`, el runtime NO debe presentarse como listo para flujo offline core.
Además, un pack `release` no puede seguir declarando `external_artifacts_required`.

## Ownership sugerido

- Producto/app: define qué capacidades entran en “core offline”.
- Release engineering: inyecta artifacts, recalcula checksums y publica installers.
- Maintainers de OCR/NLP: validan licencias, tamaño y compatibilidad de los modelos/caches incluidos.
