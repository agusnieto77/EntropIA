# Desktop App Specification

## Purpose

Defines the Tauri 2 + Svelte SPA shell that serves as the primary user-facing application, including window configuration, hot reload, and cross-package rebuild behavior.

## Requirements

### Requirement: App Launch

The desktop application MUST launch a native Tauri window rendering a Svelte root component. The Svelte frontend MUST be a plain SPA (Vite + Svelte, NOT SvelteKit).

#### Scenario: First launch renders root component

- GIVEN `apps/desktop` is built and the Tauri binary is executed
- WHEN the application window opens
- THEN a Svelte root component renders inside the webview
- AND the window title is "EntropIA"
- AND the Collections List view is displayed as the default route

#### Scenario: Dev mode connects to Vite dev server

- GIVEN `tauri.conf.json` sets `devUrl` to `http://localhost:5173`
- WHEN `pnpm tauri dev` is run inside `apps/desktop`
- THEN the Tauri window loads the Vite dev server URL

### Requirement: Hot Reload — Local Sources

Changes to files in `apps/desktop/src/` MUST trigger Vite HMR without requiring an app restart or full page reload.

#### Scenario: Svelte component change reflects instantly

- GIVEN the app is running in dev mode via `pnpm dev`
- WHEN a Svelte component in `apps/desktop/src/` is modified and saved
- THEN the change is reflected in the running app within 2 seconds
- AND no full page reload occurs (HMR patch)

### Requirement: Hot Reload — Cross-Package

Changes to `packages/ui` source files SHOULD trigger a Vite rebuild in `apps/desktop` during development, reflecting the updated components.

#### Scenario: UI package change triggers desktop rebuild

- GIVEN `apps/desktop` imports components from `@entropia/ui`
- AND the app is running in dev mode
- WHEN a component in `packages/ui/src/` is modified and saved
- THEN Vite detects the dependency change and rebuilds
- AND the updated component is reflected in the running app

### Requirement: Window Configuration

The Tauri window MUST be configured with a minimum size, a default size, and a title. Window decorations SHOULD use the OS-native style.

#### Scenario: Default window properties

- GIVEN `tauri.conf.json` defines window configuration
- WHEN the app launches
- THEN the window title is "EntropIA"
- AND the default size is at least 1024x768
- AND the minimum size is at least 800x600
- AND OS-native window decorations are used

### Requirement: Router Integration

`App.svelte` MUST use the `NavigationStore` to conditionally render the active view. The three views (Collections List, Collection Detail, Item Detail) MUST be rendered based on `NavigationStore.current`.

#### Scenario: App renders active view from navigation state

- GIVEN the `NavigationStore` current view is `{ name: 'collections' }`
- WHEN `App.svelte` renders
- THEN the Collections List view component is displayed

#### Scenario: Navigation state change switches view

- GIVEN the app is showing Collections List
- WHEN the navigation state changes to `{ name: 'collection-detail', collectionId: '123' }`
- THEN the Collection Detail view renders with the specified collection

### Requirement: Layout Shell

`App.svelte` MUST provide a layout shell with a top bar (showing current view context and back button) and a main content area. The layout MUST replace the placeholder content from Fase 0.

#### Scenario: Layout replaces placeholder

- GIVEN the app launches
- WHEN the main view renders
- THEN the Fase 0 placeholder ("Ready. Database initialized.") is no longer shown
- AND a layout with navigation bar and content area is displayed

#### Scenario: Back button visible when not at root

- GIVEN the user is on Collection Detail view
- WHEN the layout renders
- THEN a back button is visible in the top bar
