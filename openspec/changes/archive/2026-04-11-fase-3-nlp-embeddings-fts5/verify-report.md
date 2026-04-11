# Verification Report

**Change**: `fase-3-nlp-embeddings-fts5`
**Date**: 2026-04-11
**Mode**: Standard (Strict TDD not active)
**Verifier**: sdd-verify sub-agent (focused re-verification after 4 fixes)
**Previous report**: PASS WITH WARNINGS (4 criticals)

---

## Focused Re-Verification: 4 Applied Fixes

### Check 1: EmbeddingRepo.knnSearch ✅ PASS

- **File**: `packages/store/src/repos/embedding.repo.ts` (lines 98–131)
- `knnSearch(itemId: string, limit = 5): Promise<Array<{ itemId: string; distance: number }>>` — exists with correct signature
- Uses `cosineSimilarity()` helper, excludes self (`WHERE item_id != ?`), sorts ascending by distance (most similar first), respects `limit`
- **Tests**: `packages/store/src/repos/embedding.repo.test.ts` — `describe('knnSearch')` with **3 tests**:
  1. `returns empty array when item has no embedding`
  2. `returns similar items sorted by distance (ascending)`
  3. `respects the limit parameter`

### Check 2: FTS5 searchByText ✅ PASS

- **File**: `packages/store/src/repos/item.repo.ts` (lines 68–109)
- `searchByText` now checks `this.ftsRepo` first; calls `ftsRepo.search(query, 50)`; if results found, fetches those items via Drizzle OR chain
- Falls back to `SQL LIKE` on title+metadata when FTS5 returns 0 results or rawClient not available
- Spec compliance: FTS5 MATCH is tried first ✅; LIKE retained as fallback (spec-aligned behavior)

### Check 3: OcrStore onComplete ✅ PASS

- **`apps/desktop/src/lib/ocr.ts`**: `OcrStoreOptions.onComplete?: (assetId: string) => void` defined (line 65); called in `startListening → listen('ocr:complete', ...)` handler (line 103): `this.onComplete?.(p.asset_id)`
- **`apps/desktop/src/views/ItemView.svelte`** (lines 36–44): OcrStore instantiated with `onComplete: (assetId) => { void indexFts(itemId).catch(...) }` — auto-triggers FTS indexing after OCR completion

### Check 4: Test Count ✅ PASS

| Package           | Tests   | Status        |
| ----------------- | ------- | ------------- |
| @entropia/store   | 110     | ✅ all passed |
| @entropia/ui      | 57      | ✅ all passed |
| @entropia/desktop | 69      | ✅ all passed |
| **TOTAL**         | **236** | ✅ 0 failures |

Previous count was 230. Gain of **+6** (3 new knnSearch tests + incremental desktop/nlp tests).

### Check 5: Typecheck ❌ FAIL — 2 TypeScript errors

**`@entropia/store:typecheck` exits with code 2.**

Errors in `packages/store/src/repos/item.repo.ts` lines 85–86:

```
src/repos/item.repo.ts(85,20): error TS2352: Conversion of type 'string' to type 'SQL<unknown>'
  may be a mistake because neither type sufficiently overlaps with the other.

src/repos/item.repo.ts(86,34): error TS2769: No overload matches this call.
  Type 'SQL<unknown> | undefined' is not assignable to type 'SQL<unknown>'.
```

**Root cause**: The `ids.reduce()` call in the OR-chain builder uses incorrect types.

- `ids` is `string[]`; `Array.prototype.reduce` infers `acc` as `string` (array element type) unless overloaded correctly
- `or(acc, eq(...))` returns `SQL<unknown> | undefined` but `reduce`'s initial value typing confuses TS
- The `as ReturnType<typeof eq>` cast is not wide enough to silence the type mismatch

**Fix needed**:

```typescript
// Replace lines 83-88 with a properly typed OR chain
const idConditions = ids.map((id) => eq(items.id, id))
const idFilter = idConditions.length === 1 ? idConditions[0]! : or(...idConditions)!
```

### Check 6: Lint ✅ PASS

- `@entropia/store:lint`: 0 errors, 0 warnings
- `@entropia/ui:lint`: 0 errors, 0 warnings
- `@entropia/desktop:lint`: 0 errors, **2 pre-existing warnings** (unrelated to Fase 3):
  - `file-import.test.ts:87` — `no-explicit-any`
  - `keyboard.test.ts:3` — `no-unused-vars`

---

## Completeness

| Metric               | Value |
| -------------------- | ----- |
| Tasks total          | 48    |
| Tasks complete [x]   | 46    |
| Tasks incomplete [ ] | 2     |

**Still incomplete** (deferred, not blockers):

- `[ ] 5.4` — `cargo test` Rust unit tests (deferred, requires Rust toolchain)
- `[ ] 5.5` — Manual smoke test (deferred, requires live app)

---

## Build & Tests Execution

**Build (typecheck)**: ❌ Failed — 2 TS errors in `packages/store/src/repos/item.repo.ts`

**Tests**: ✅ 236 passed / ❌ 0 failed / ⚠️ 0 skipped

**Coverage**: Not configured — N/A

**Lint**: ✅ 0 errors (2 pre-existing warnings in unrelated test files)

---

## Spec Compliance Matrix (Updated for fixed items)

### data-store-nlp-delta.md — Updated rows

| Requirement                   | Scenario                             | Test                                                                       | Result       |
| ----------------------------- | ------------------------------------ | -------------------------------------------------------------------------- | ------------ |
| Embedding Repository StoreApi | knnSearch returns nearest neighbors  | `embedding.repo.test.ts > knnSearch > returns sorted by distance`          | ✅ COMPLIANT |
| Item Repository               | searchByText uses FTS5 via fts_items | `item.repo.test.ts > searchByText > prefers FTS5 when rawClient available` | ✅ COMPLIANT |
| OCR → FTS5 auto-trigger       | onComplete fires indexFts after OCR  | `ocr.test.ts > OcrStore > onComplete callback called on ocr:complete`      | ✅ COMPLIANT |

_All other compliance rows from previous report remain unchanged._

---

## Issues Found

### CRITICAL (must fix before archive)

1. **TypeScript typecheck fails — `item.repo.ts` lines 85–86** (`packages/store/src/repos/item.repo.ts`)
   - The OR-chain reducer introduced by fix #3 causes 2 TS errors (type mismatch in `Array.reduce`)
   - `pnpm typecheck` exits with code 2
   - **Fix**: Replace the `reduce` pattern with `or(...ids.map(id => eq(items.id, id)))!`

2. **`Re-embed on extraction update` not implemented** (pre-existing, carried from previous report)
   - Spec (`embeddings.md`): "When an extraction is saved or updated, MUST enqueue a re-embedding job"
   - `ExtractionRepo.upsert` does not call `NlpQueue.submit(ComputeEmbedding)`
   - No hook between extraction save and NLP queue

### WARNING (should fix)

1. **Entity type casing mismatch** (pre-existing)
   - Spec uses uppercase: `PERSON`, `PLACE`, etc.
   - Implementation uses lowercase: `'person'`, `'place'`, etc.
   - Consistent internally but deviates from spec text

2. **`NlpProgressPayload` missing `job_id` field** (pre-existing)
   - Spec requires `nlp:progress` to include `job_id`; actual payload uses `item_id + job type`

3. **`FtsRepo.search` does not accept `collectionId` filter parameter** (pre-existing)
   - Current signature: `search(query: string, limit = 20)` — no collection scoping at TS layer

4. **Pre-existing Svelte `state_referenced_locally` warnings** (7 warnings in ui components)

### SUGGESTION

1. **Rust `uuid_v4()` is a non-standard UUID implementation** in `ner.rs`
2. **FtsRepo TS tests use mock DbClient** — real in-memory SQLite would improve confidence
3. **`fts_search` and `similar_items` Tauri commands return a "note" stub** — document as MVP limitation

---

## Fix Status vs Previous Report

| Critical (from prev report)                 | Fixed?                  | Notes                                            |
| ------------------------------------------- | ----------------------- | ------------------------------------------------ |
| `knnSearch` missing                         | ✅ Fixed                | Implemented + 3 tests                            |
| `vec_items` not in migration 0005           | ✅ Fixed                | Migration file now documents runtime requirement |
| `searchByText` uses LIKE only               | ✅ Fixed                | FTS5 first, LIKE fallback                        |
| Re-embed on extraction update               | ❌ Not fixed            | Still missing (see CRITICAL #2)                  |
| **NEW: TS typecheck error in item.repo.ts** | ❌ Introduced by fix #3 | OR-chain reducer typing broken                   |

---

## Verdict

### **FAIL**

3 of the 4 original criticals are resolved. However:

- Fix #3 (`searchByText` FTS5 upgrade) introduced a **TypeScript typecheck error** that causes `pnpm typecheck` to exit with code 2. This is a NEW critical introduced by the fix itself.
- Fix #2 (migration docs) addressed the spec gap but did not add `vec_items` to the migration SQL — this was accepted as "runtime creation is sufficient" in the fix session.
- Critical #4 (re-embed on update) remains unimplemented.

**Two actions required before archive**:

1. Fix `item.repo.ts` OR-chain typing error (1-line fix, no functional change)
2. Decide on re-embed-on-update: either implement or explicitly mark as out-of-scope in the spec

Once the typecheck error is fixed, the change can be re-verified as **PASS WITH WARNINGS** (remaining warnings are non-blocking).
