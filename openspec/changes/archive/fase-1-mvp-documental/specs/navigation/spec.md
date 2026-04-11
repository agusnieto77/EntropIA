# Navigation Specification

## Purpose

Defines URL-less, state-driven navigation between the three main views of the EntropIA desktop application.

## Requirements

### Requirement: View Definitions

The application MUST support exactly three views: Collections List, Collection Detail, and Item Detail. View switching MUST be driven by a reactive `NavigationStore` using Svelte 5 `$state` runes.

#### Scenario: App starts on collections list

- GIVEN the application launches
- WHEN the main view renders
- THEN the Collections List view is displayed

#### Scenario: Navigate to collection detail

- GIVEN the user is on the Collections List
- WHEN they select a collection
- THEN the Collection Detail view renders with that collection's items

#### Scenario: Navigate to item detail

- GIVEN the user is on the Collection Detail view
- WHEN they select an item
- THEN the Item Detail view renders with that item's document viewer, metadata, and notes

### Requirement: Back/Forward History

The `NavigationStore` MUST maintain a history stack. Navigating forward MUST push the current view onto the stack. Going back MUST pop the previous view from the stack.

#### Scenario: Go back to previous view

- GIVEN the user navigated: Collections List → Collection Detail → Item Detail
- WHEN the user triggers "go back"
- THEN the Collection Detail view is restored

#### Scenario: Back at root is no-op

- GIVEN the user is on the Collections List with an empty history stack
- WHEN the user triggers "go back"
- THEN nothing happens
- AND the Collections List remains displayed

### Requirement: Keyboard Navigation

The Escape key MUST navigate back one level (equivalent to the back action).

#### Scenario: Escape goes back

- GIVEN the user is on the Item Detail view
- WHEN they press the Escape key
- THEN the Collection Detail view is restored

#### Scenario: Escape on collections list is no-op

- GIVEN the user is on the Collections List
- WHEN they press Escape
- THEN nothing happens

### Requirement: No URL Dependency

Navigation MUST NOT depend on browser URL, hash routing, or any external router library. All state MUST be internal to the `NavigationStore` class.

#### Scenario: Navigation works without URL bar

- GIVEN the app runs in a Tauri webview with no visible URL bar
- WHEN the user navigates between views
- THEN all transitions work via internal state
- AND no URL changes occur
