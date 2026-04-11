# Delta for Desktop App

## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: App Launch

The desktop application MUST launch a native Tauri window rendering a Svelte root component. The Svelte frontend MUST be a plain SPA (Vite + Svelte, NOT SvelteKit).
(Previously: No change to base requirement — but the root component now includes router and layout shell instead of a placeholder card.)

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
