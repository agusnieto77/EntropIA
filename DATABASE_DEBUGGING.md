# Database Debugging de EntropIA

Guía operativa para diagnosticar problemas en la base SQLite de EntropIA sin dar vueltas.

## Base activa

```text
C:\Users\agusn\AppData\Roaming\com.entropia.desktop\entropia.sqlite
```

## Abrir la base

```powershell
sqlite3 "C:\Users\agusn\AppData\Roaming\com.entropia.desktop\entropia.sqlite"
```

## Filosofía de debugging

No mires tablas aisladas como si fueran cajitas mágicas. Pensá el flujo:

```text
collection -> item -> asset -> procesamiento -> enriquecimiento -> búsqueda
```

Traducido a tablas:

```text
collections -> items -> assets -> jobs / extractions / transcriptions / layouts
collections / items / assets -> llm_results
items -> notes / entities / triples / item_topics
item_topics -> topics
items/assets -> vec_items / vec_assets / embeddings_fallback
items -> fts_items
```

---

## Flujo de diagnóstico rápido

### 1. Verificar existencia de colección, ítem y asset

```sql
SELECT * FROM collections ORDER BY created_at DESC;

SELECT i.id, i.title, c.name AS collection_name
FROM items i
JOIN collections c ON c.id = i.collection_id
ORDER BY i.created_at DESC;

SELECT *
FROM assets
ORDER BY created_at DESC;
```

### 2. Verificar jobs

```sql
SELECT id, type, status, asset_id, error, created_at, updated_at
FROM jobs
ORDER BY updated_at DESC;
```

### 3. Verificar persistencia de resultados

```sql
SELECT asset_id, method, confidence, created_at
FROM extractions
ORDER BY created_at DESC;

SELECT asset_id, language, duration_ms, model, confidence, created_at
FROM transcriptions
ORDER BY created_at DESC;

SELECT asset_id, model, created_at
FROM layouts
ORDER BY created_at DESC;

SELECT target_id, target_type, job_type, created_at
FROM llm_results
ORDER BY created_at DESC;
```

### 4. Verificar enriquecimiento semántico

```sql
SELECT item_id, COUNT(*) AS entities
FROM entities
GROUP BY item_id
ORDER BY entities DESC;

SELECT item_id, COUNT(*) AS triples
FROM triples
GROUP BY item_id
ORDER BY triples DESC;

SELECT item_id, COUNT(*) AS notes
FROM notes
GROUP BY item_id
ORDER BY notes DESC;
```

### 5. Verificar indexación/búsqueda

```sql
SELECT item_id, title
FROM fts_items
LIMIT 20;

SELECT item_id, length(embedding) AS bytes
FROM vec_items
LIMIT 20;

SELECT asset_id, item_id, length(embedding) AS bytes
FROM vec_assets
LIMIT 20;
```

---

## Problema -> dónde mirar -> query

### A. “No aparece una colección”

Mirar:
- `collections`

```sql
SELECT id, name, description, created_at, updated_at
FROM collections
ORDER BY created_at DESC;
```

### B. “No aparece un ítem”

Mirar:
- `items`
- `collections`

```sql
SELECT i.id, i.title, i.collection_id, c.name
FROM items i
LEFT JOIN collections c ON c.id = i.collection_id
ORDER BY i.created_at DESC;
```

### C. “El ítem existe, pero no tiene assets”

Mirar:
- `assets`

```sql
SELECT id, item_id, path, type, size, sort_index, created_at
FROM assets
WHERE item_id = 'ITEM_ID_AQUI'
ORDER BY sort_index, created_at;
```

### D. “El asset está, pero OCR/transcripción no corrió”

Mirar:
- `jobs`
- `extractions`
- `transcriptions`

```sql
SELECT a.id, a.path, j.type, j.status, j.error
FROM assets a
LEFT JOIN jobs j ON j.asset_id = a.id
WHERE a.id = 'ASSET_ID_AQUI'
ORDER BY j.updated_at DESC;
```

### E. “OCR High no dejó layout”

Mirar:
- `layouts`
- `extractions`

```sql
SELECT asset_id, model, image_width, image_height, created_at
FROM layouts
WHERE asset_id = 'ASSET_ID_AQUI';
```

### F. “No aparecen entidades o triples”

Mirar:
- `entities`
- `triples`

```sql
SELECT id, entity_type, value, confidence, source, model_name
FROM entities
WHERE item_id = 'ITEM_ID_AQUI'
ORDER BY confidence DESC;

SELECT subject, predicate, object, created_at
FROM triples
WHERE item_id = 'ITEM_ID_AQUI'
ORDER BY created_at DESC;
```

### G. “No aparecen topics”

Mirar:
- `item_topics`
- `topics`

```sql
SELECT t.name
FROM item_topics it
JOIN topics t ON t.id = it.topic_id
WHERE it.item_id = 'ITEM_ID_AQUI';
```

### H. “La búsqueda FTS no devuelve nada”

Mirar:
- `fts_items`

```sql
SELECT item_id, title, extracted_text
FROM fts_items
WHERE fts_items MATCH 'termino';
```

### I. “La similitud/embeddings no funciona”

Mirar:
- `vec_items`
- `vec_assets`
- `embeddings_fallback`

```sql
SELECT item_id, length(embedding) AS bytes
FROM vec_items
WHERE item_id = 'ITEM_ID_AQUI';

SELECT asset_id, item_id, length(embedding) AS bytes
FROM vec_assets
WHERE asset_id = 'ASSET_ID_AQUI';
```

### J. “El resultado LLM quedó mezclado entre asset/item/collection o desapareció”

Mirar:
- `llm_results`

```sql
SELECT id, target_id, target_type, job_type, created_at, result
FROM llm_results
WHERE target_id = 'TARGET_ID_AQUI'
ORDER BY created_at DESC;

SELECT id, target_id, target_type, job_type, created_at
FROM llm_results
WHERE created_at < 1000000000000
ORDER BY created_at ASC;
```

---

## Consultas de auditoría relacional

### Assets huérfanos respecto de items

```sql
SELECT a.*
FROM assets a
LEFT JOIN items i ON i.id = a.item_id
WHERE i.id IS NULL;
```

### Items huérfanos respecto de collections

```sql
SELECT i.*
FROM items i
LEFT JOIN collections c ON c.id = i.collection_id
WHERE c.id IS NULL;
```

### Notes apuntando a assets inexistentes

```sql
SELECT n.*
FROM notes n
LEFT JOIN assets a ON a.id = n.asset_id
WHERE n.asset_id IS NOT NULL
  AND a.id IS NULL;
```

### Entities con asset_id roto

```sql
SELECT e.*
FROM entities e
LEFT JOIN assets a ON a.id = e.asset_id
WHERE e.asset_id IS NOT NULL
  AND a.id IS NULL;
```

### Triples con asset_id roto

```sql
SELECT t.*
FROM triples t
LEFT JOIN assets a ON a.id = t.asset_id
WHERE t.asset_id IS NOT NULL
  AND a.id IS NULL;
```

### Vec assets con references dudosas

```sql
SELECT va.*
FROM vec_assets va
LEFT JOIN assets a ON a.id = va.asset_id
LEFT JOIN items i ON i.id = va.item_id
WHERE a.id IS NULL OR i.id IS NULL;
```

### LLM results con target roto o legacy no migrado

```sql
SELECT lr.*
FROM llm_results lr
LEFT JOIN assets a ON lr.target_type = 'asset' AND a.id = lr.target_id
LEFT JOIN items i ON lr.target_type = 'item' AND i.id = lr.target_id
LEFT JOIN collections c ON lr.target_type = 'collection' AND c.id = lr.target_id
WHERE (lr.target_type = 'asset' AND a.id IS NULL)
   OR (lr.target_type = 'item' AND i.id IS NULL)
   OR (lr.target_type = 'collection' AND c.id IS NULL)
   OR lr.target_type = 'unknown';
```

---

## Consultas para entender cobertura del pipeline

### Qué assets tienen qué tipo de salida

```sql
SELECT
  a.id AS asset_id,
  a.type,
  CASE WHEN e.asset_id IS NOT NULL THEN 'yes' ELSE 'no' END AS extraction,
  CASE WHEN t.asset_id IS NOT NULL THEN 'yes' ELSE 'no' END AS transcription,
  CASE WHEN l.asset_id IS NOT NULL THEN 'yes' ELSE 'no' END AS layout
FROM assets a
LEFT JOIN extractions e ON e.asset_id = a.id
LEFT JOIN transcriptions t ON t.asset_id = a.id
LEFT JOIN layouts l ON l.asset_id = a.id
ORDER BY a.created_at DESC;
```

### Qué ítems están enriquecidos semánticamente

```sql
SELECT
  i.id,
  i.title,
  (SELECT COUNT(*) FROM entities e WHERE e.item_id = i.id) AS entity_count,
  (SELECT COUNT(*) FROM triples t WHERE t.item_id = i.id) AS triple_count,
  (SELECT COUNT(*) FROM item_topics it WHERE it.item_id = i.id) AS topic_count,
  (SELECT COUNT(*) FROM notes n WHERE n.item_id = i.id) AS note_count
FROM items i
ORDER BY i.updated_at DESC;
```

---

## Comandos útiles de sqlite3

```sql
.tables
.schema
.schema items
.schema assets
.indexes items
.indexes assets
PRAGMA foreign_keys;
PRAGMA integrity_check;
PRAGMA quick_check;
```

## Recomendación brutalmente práctica

Cuando algo falla, no empieces por inferencias. Empezá por evidencia:

1. `assets`
2. `jobs`
3. `extractions` / `transcriptions` / `layouts`
4. `entities` / `triples` / `topics`
5. `fts_items` / `vec_*`

Es así de simple. Primero verificás persistencia. Después discutís lógica.
