# Search Specification

## Purpose

Defines text search functionality for finding items across collections by title and metadata content.

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

Search MUST query `items.title` and `items.metadata` using SQL LIKE patterns. The search MUST be case-insensitive.

#### Scenario: Search matches item title

- GIVEN items exist with titles "Acta de cabildo" and "Carta al gobernador"
- WHEN the user searches for "cabildo"
- THEN "Acta de cabildo" appears in the results
- AND "Carta al gobernador" does not

#### Scenario: Search matches metadata value

- GIVEN an item has metadata `{"author": "Mariano Moreno"}`
- WHEN the user searches for "Moreno"
- THEN that item appears in the results

#### Scenario: Search is case-insensitive

- GIVEN an item titled "Acta de Cabildo"
- WHEN the user searches for "acta"
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
