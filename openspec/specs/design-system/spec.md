# Design System Specification

## Purpose

Defines the `packages/ui` component library that provides CSS design tokens and reusable Svelte components consumed by `apps/desktop` and potentially future consumers.

## Requirements

### Requirement: CSS Design Tokens

`packages/ui` MUST define CSS Custom Properties (variables) for: colors, spacing, typography (font family, size, weight, line-height), and border radius. Tokens MUST be importable as CSS files.

#### Scenario: Tokens are available as CSS variables

- GIVEN the design token CSS files are imported in the app
- WHEN a component uses `var(--color-primary)` or `var(--spacing-md)`
- THEN the CSS variables resolve to the defined token values

#### Scenario: Token categories are organized and discoverable

- GIVEN `packages/ui/src/tokens/`
- WHEN the token source is inspected
- THEN color, spacing, typography, and radius token categories are clearly organized (single file with sections OR split files)

### Requirement: Base Components

`packages/ui` MUST export at least three base Svelte components: `Button`, `Input`, and `Card`. Each component MUST use the design tokens for styling and MUST accept props with TypeScript types.

#### Scenario: Button renders with token styles

- GIVEN a `Button` component imported from `@entropia/ui`
- WHEN rendered with default props
- THEN it uses design token CSS variables for colors, padding, and border-radius

#### Scenario: Components export TypeScript types

- GIVEN a consumer imports `Button` from `@entropia/ui`
- WHEN TypeScript checks the import
- THEN prop types are available (e.g., `variant`, `disabled`, `size`)
- AND no type errors occur in a correctly typed consumer

### Requirement: Package Consumability

`packages/ui` MUST be consumable by `apps/desktop` via the `@entropia/ui` package name using `workspace:*` protocol. Components and tokens MUST be importable from the package entry point.

#### Scenario: Desktop app imports UI components

- GIVEN `apps/desktop/package.json` lists `@entropia/ui: "workspace:*"`
- WHEN a Svelte file imports `import { Button } from '@entropia/ui'`
- THEN the component is resolved from the local workspace package
- AND it renders correctly in the desktop app

#### Scenario: CSS tokens are importable

- GIVEN the design token CSS files are part of the package exports
- WHEN `apps/desktop` imports `@entropia/ui/tokens.css` (or equivalent entry)
- THEN all CSS Custom Properties are available in the app's cascade

### Requirement: CollectionCard Component

`packages/ui` MUST export a `CollectionCard` component that displays a collection's name, description (truncated if long), item count badge, and last updated date. It MUST accept typed props.

#### Scenario: CollectionCard renders all fields

- GIVEN a collection with name, description, 15 items, and updated date
- WHEN `CollectionCard` is rendered with those props
- THEN the name, description, item count "15", and formatted date are visible

### Requirement: ItemCard Component

`packages/ui` MUST export an `ItemCard` component that displays an item's title, a thumbnail of its first asset (or placeholder), and a metadata preview.

#### Scenario: ItemCard with asset thumbnail

- GIVEN an item with title and a first asset image URL
- WHEN `ItemCard` is rendered
- THEN the title and thumbnail image are visible

#### Scenario: ItemCard without assets

- GIVEN an item with no assets
- WHEN `ItemCard` is rendered
- THEN a placeholder icon is shown instead of a thumbnail

### Requirement: DocumentViewer Component

`packages/ui` MUST export a `DocumentViewer` component that accepts an `asset` prop and renders images via `<img>` or PDFs via `pdfjs-dist` based on the asset's MIME type.

#### Scenario: Renders image asset

- GIVEN an asset with `mime_type = 'image/jpeg'`
- WHEN `DocumentViewer` is rendered
- THEN an `<img>` element displays the image

#### Scenario: Renders PDF asset

- GIVEN an asset with `mime_type = 'application/pdf'`
- WHEN `DocumentViewer` is rendered
- THEN the PDF is rendered via pdfjs-dist with page controls

### Requirement: SearchBar Component

`packages/ui` MUST export a `SearchBar` component with a debounced text input (300ms). It MUST emit a search event with the current query string.

#### Scenario: Debounced search event

- GIVEN a `SearchBar` is rendered
- WHEN the user types "cab" and pauses for 300ms
- THEN a search event fires with value "cab"

### Requirement: MetadataEditor Component

`packages/ui` MUST export a `MetadataEditor` component that renders key-value pairs as editable fields. It MUST support adding new pairs, editing values, and removing pairs.

#### Scenario: Add key-value pair

- GIVEN a `MetadataEditor` with empty metadata
- WHEN the user adds key "date" with value "1810"
- THEN the metadata state includes `{ "date": "1810" }`

#### Scenario: Remove key-value pair

- GIVEN a `MetadataEditor` with metadata `{ "date": "1810", "author": "Moreno" }`
- WHEN the user removes the "author" key
- THEN the metadata state updates to `{ "date": "1810" }`
