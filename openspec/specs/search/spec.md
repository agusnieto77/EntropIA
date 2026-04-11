# Search Specification

## Purpose

Defines text search functionality for finding items across collections by title, metadata content, and extracted text using FTS5 full-text search.

## Requirements

### Requirement: Search Availability

A search bar MUST be available on the collections list view and the collection detail view.

#### Scenario: Search bar on collections list

- GIVEN the user is on the collections list view
- WHEN the view loads
- THEN a search bar is visible and functional

#### Scenario: Search bar on collection detail

- GIVEN the user is viewing a collection's items
- WHEN the view loads
- THEN a search bar is visible and functional

### Requirement: Search Execution

Search MUST query `items.title`, `items.metadata`, and `extractions.text_content` using FTS5 MATCH via the `fts_items` virtual table. The search MUST be case-insensitive and accent-folded. Results MUST be ordered by relevance rank. The `sanitizeFts5Query()` utility MUST be applied to all user input before the MATCH expression is constructed.

(Previously: searched only `items.title` and `items.metadata` using SQL LIKE patterns)

#### Scenario: Search matches item title

- GIVEN items exist with titles "Acta de cabildo" and "Carta al gobernador"
- WHEN the user searches for "cabildo"
- THEN "Acta de cabildo" appears in the results
- AND "Carta al gobernador" does not

#### Scenario: Search matches extracted text

- GIVEN an item whose extraction contains "Gobernador Intendente de Córdoba"
- WHEN the user searches for "Intendente"
- THEN that item appears in the results

#### Scenario: Search matches metadata value

- GIVEN an item has metadata `{"author": "Mariano Moreno"}`
- WHEN the user searches for "Moreno"
- THEN that item appears in the results

#### Scenario: Search is case-insensitive and accent-folded

- GIVEN an item titled "Acción de Gracias"
- WHEN the user searches for "accion"
- THEN the item appears in the results

### Requirement: Debounced Input

Search MUST be debounced at 300ms — results update as the user types, but queries fire only after 300ms of inactivity.

#### Scenario: Rapid typing triggers single query

- GIVEN the user types "cab" over 200ms
- WHEN 300ms pass after the last keystroke
- THEN a single search query executes for "cab"

### Requirement: Empty Results State

The system MUST show an empty state when no items match the search query.

#### Scenario: No results found

- GIVEN the user searches for "xyznonexistent"
- WHEN no items match
- THEN an empty state message is displayed indicating no results

### Requirement: Search Scope

On the collections list view, search MUST query items across ALL collections. On the collection detail view, search MUST be scoped to items within that collection only.

#### Scenario: Global search on collections list

- GIVEN items exist in collections A and B
- WHEN the user searches from the collections list
- THEN results include matching items from both collections

#### Scenario: Scoped search on collection detail

- GIVEN items exist in collections A and B
- WHEN the user searches from collection A's detail view
- THEN results include only matching items from collection A

### Requirement: FTS5 Virtual Table

The system MUST maintain an FTS5 virtual table `fts_items` that indexes the `title`, `metadata`, and `extracted_text` columns for all items. The table MUST use the `unicode61` tokenizer. The virtual table MUST be created by migration `0004_fts5`.

#### Scenario: FTS5 table exists after migration

- GIVEN a database with migrations 0001–0003 applied
- WHEN migration 0004 is applied
- THEN the `fts_items` virtual table exists
- AND it has columns `item_id`, `title`, `metadata`, `extracted_text`

#### Scenario: FTS5 uses unicode61 tokenizer

- GIVEN the `fts_items` table is created
- WHEN its schema is inspected
- THEN the tokenizer is `unicode61`
- AND accent-folding is active (e.g., "Acción" matches "accion")

---

### Requirement: FTS5 Sync on Extraction Save

The system MUST keep `fts_items` in sync with the `items` and `extractions` tables. When an extraction is created or updated, the corresponding row in `fts_items` MUST be inserted or replaced. When an item is deleted, its row in `fts_items` MUST be deleted.

#### Scenario: New extraction triggers FTS5 upsert

- GIVEN an item has no entry in `fts_items`
- WHEN a new extraction is saved for that item's asset
- THEN a row is inserted into `fts_items` with the item's title, metadata, and extracted text

#### Scenario: Updated extraction refreshes FTS5 row

- GIVEN an item already has an entry in `fts_items`
- WHEN the item's extraction is updated via `upsert`
- THEN the `fts_items` row is replaced with the new extracted_text
- AND the old content is no longer indexed

#### Scenario: Deleted item removes FTS5 row

- GIVEN an item has a row in `fts_items`
- WHEN the item is deleted from `items`
- THEN the corresponding `fts_items` row is also deleted

---

### Requirement: FTS5 Query Sanitization

The system MUST sanitize user input before constructing an FTS5 MATCH expression. The `sanitizeFts5Query()` utility MUST wrap each whitespace-delimited token in double quotes and strip any characters that are FTS5 operators (`(`, `)`, `"`, `-`, `*`, `^`, `:`, `,`, `.`).

#### Scenario: Plain text query sanitized

- GIVEN a user query `acta cabildo`
- WHEN `sanitizeFts5Query` is called
- THEN it returns `"acta" "cabildo"`

#### Scenario: Special characters stripped

- GIVEN a user query `acta AND (cabildo)`
- WHEN `sanitizeFts5Query` is called
- THEN operators and parentheses are stripped
- AND the result is a safe quoted token list

#### Scenario: Empty query returns empty string

- GIVEN a user query of empty string or only whitespace
- WHEN `sanitizeFts5Query` is called
- THEN it returns an empty string
- AND no MATCH query is executed

---

### Requirement: FTS5 Search Execution

`ItemRepo.searchByText(term, collectionId?)` MUST use FTS5 MATCH instead of SQL LIKE. Results MUST include a `rank` score. Results MUST be ordered by rank (most relevant first). Results MAY include a BM25 snippet.

#### Scenario: Basic keyword match returns ranked results

- GIVEN items indexed in `fts_items` with extracted text
- WHEN `searchByText('cabildo')` is called
- THEN items containing "cabildo" are returned
- AND results are ordered by rank score descending

#### Scenario: No results returns empty array

- GIVEN a populated `fts_items` table
- WHEN `searchByText('xyznonexistentterm')` is called
- THEN an empty array is returned
- AND no error is thrown

#### Scenario: Scoped search filters by collection

- GIVEN items in collection A and collection B are both indexed
- WHEN `searchByText('cabildo', collectionIdA)` is called
- THEN only items from collection A appear in results
