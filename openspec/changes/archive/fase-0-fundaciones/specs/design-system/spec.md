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

#### Scenario: Token files are organized by category

- GIVEN `packages/ui/src/tokens/`
- WHEN the directory is inspected
- THEN separate CSS files exist for colors, typography, and spacing (at minimum)

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
