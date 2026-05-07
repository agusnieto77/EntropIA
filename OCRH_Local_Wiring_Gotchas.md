# OCRH Local Wiring And Gotchas

Guia corta para no romper de nuevo el cableado local de `OCRH` con `PaddleOCR-VL`.

## Respuesta corta

El wiring correcto de OCRH local es este:

1. El backend debe resolver un Python que pueda **inicializar realmente** `PaddleOCRVL(...)`.
2. El managed venv debe instalar `paddleocr[doc-parser]` **y** `paddlepaddle`.
3. Esas instalaciones deben correr como **specs separados** en `uv pip install`.
4. Si OCRH local falla, debe emitir **error explícito** al frontend.
5. No se debe degradar silenciosamente a OCR plano cuando el usuario pidió `OCRH` local.

## Quick path

1. Verificá el Python resuelto en logs: `Python resolver hit (paddle_vl, source=...)`.
2. Verificá que ese Python pueda ejecutar:

```python
from paddleocr import PaddleOCRVL
PaddleOCRVL(device='cpu', use_doc_orientation_classify=False, use_doc_unwarping=False, use_layout_detection=True)
print('ok')
```

3. Si eso falla, revisá el venv gestionado antes de tocar OCRH.
4. Si eso funciona pero la UI falla, revisá propagación de `ocr:error`.

## Cableado correcto

| Capa | Regla correcta |
|---|---|
| Dependency registry | `PaddleOCR` no se valida con import superficial; se valida con inicialización real de `PaddleOCRVL(...)` |
| Managed install | `paddleocr[doc-parser]` y `paddlepaddle` se instalan por separado |
| Python discovery | Se puede priorizar `deps_venv_python_path` solo si pasa el probe real |
| OCR backend | OCRH local debe devolver error explícito si PaddleVL falla |
| Frontend | Debe mostrar `ocr:error`, no asumir éxito si el job se degradó |

## Errores a evitar

### 1. Probe superficial

NO hacer esto:

```python
import paddleocr
print('ok')
```

Tampoco alcanza con esto:

```python
from paddleocr import PaddleOCRVL
print('ok')
```

Eso puede pasar aunque falte `paddlepaddle` y la pipeline real reviente al inicializar.

HAY QUE validar esto:

```python
from paddleocr import PaddleOCRVL
PaddleOCRVL(device='cpu', use_doc_orientation_classify=False, use_doc_unwarping=False, use_layout_detection=True)
print('ok')
```

### 2. Instalar múltiples paquetes en un solo spec

NO hacer esto con `uv pip install`:

```text
paddlepaddle>=3.2.0,<3.3.0 paddleocr[doc-parser]>=3.0.0,<3.6.0
```

Eso rompe el parseo.

HAY QUE hacer esto:

1. `paddleocr[doc-parser]>=3.0.0,<3.6.0`
2. `paddlepaddle>=3.2.0,<3.3.0`

como instalaciones separadas.

### 3. Confiar ciegamente en el managed venv

NO asumir que `deps_venv_python_path` es válido solo porque existe.

Si ese Python no pasa la inicialización real de `PaddleOCRVL(...)`, secuestra OCRH hacia un entorno roto.

### 4. Fallback silencioso a OCR plano

NO hacer que OCRH local caiga silenciosamente a OCR común.

Eso produce el peor escenario posible:

- el usuario cree que OCRH funcionó
- el layout premium no refleja la realidad
- el diagnóstico se vuelve confuso

Si el usuario pidió `OCRH`, y `PaddleOCR-VL` falla, hay que devolver error real.

### 5. Leer mal los timeouts del startup check

NO asumir que esto significa que OCRH está roto:

```text
[deps/checks] global probe timeout ... marking remaining deps Unknown
```

El probe real de PaddleOCR-VL puede tardar varios minutos en CPU.

`Unknown` en `deps_check_all` puede ser un falso negativo de reporting, no un problema real del runtime.

## Señales de que el wiring está sano

Buscá estas evidencias en logs:

```text
[paddle_vl] Python resolver hit (paddle_vl, source=managed_venv): ...
[OCR] High OCR mode available via PaddleOCR-VL
[paddle_vl] Complete: <n> blocks, <n> regions
[OCRH] PaddleVL detected <n> blocks
```

## Archivos clave

- `apps/desktop/src-tauri/src/deps/registry.rs`
  define el probe real y los specs de instalación
- `apps/desktop/src-tauri/src/deps/install.rs`
  instala múltiples specs por dependencia
- `apps/desktop/src-tauri/src/deps/checks.rs`
  timeouts y estado de probes
- `apps/desktop/src-tauri/src/python_discovery.rs`
  decide qué Python usa OCRH
- `apps/desktop/src-tauri/src/ocr/mod.rs`
  propaga errores explícitos de OCRH
- `apps/desktop/src-tauri/src/ocr/paddle_vl.rs`
  subprocess real de `PaddleOCR-VL`

## Checklist

- [ ] El Python resuelto para `paddle_vl` es el esperado
- [ ] Ese Python puede inicializar `PaddleOCRVL(...)`
- [ ] El venv tiene `paddlepaddle` además de `paddleocr`
- [ ] OCRH no degrada silenciosamente a OCR plano
- [ ] La UI muestra `ocr:error` cuando falla OCRH
- [ ] No se interpreta `Unknown` de deps como prueba automática de rotura

## Siguiente mejora natural

Separar el probe liviano de startup del probe pesado de validación profunda para que `deps_check_all` no marque `Unknown` falsos mientras OCRH real funciona bien.

## Mejora pendiente en deps_check_all

Esto también quedó como deuda técnica explícita:

- `deps_check_all` hoy puede mostrar `Unknown` falsos cuando el probe pesado de `PaddleOCR-VL` tarda mucho en inicializar en CPU.
- El cableado de OCRH puede estar sano aunque el startup check mienta por timeout agregado.

### Qué hacer

Hay que dejar fino `deps_check_all` con una de estas estrategias:

1. usar un probe liviano para startup y reservar el probe pesado para validación profunda
2. usar timeout global más inteligente, específico para `PaddleOCR`
3. no degradar a `Unknown` automáticamente cuando el único cuello es un probe pesado conocido

### Qué NO hacer

- NO usar el probe pesado de `PaddleOCRVL(...)` como único criterio rápido de startup si el timeout global sigue siendo corto
- NO mostrar `Unknown` como si fuera evidencia de rotura real del runtime
- NO volver a simplificar el probe hasta `import paddleocr`, porque eso reintroduce falsos positivos
