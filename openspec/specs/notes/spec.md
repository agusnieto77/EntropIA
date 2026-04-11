# Notes Specification

## Purpose

Defines the note-taking feature that allows users to annotate items with plain text notes.

## Requirements

### Requirement: Create Note

The system MUST allow users to add a plain text note to an item. Multiple notes per item MUST be supported. The system MUST generate a unique ID and set a `created_at` timestamp.

#### Scenario: Add note to item

- GIVEN the user is viewing an item's detail
- WHEN they type a note and submit it
- THEN a note record is created linked to the item
- AND the note appears in the notes list

#### Scenario: Add multiple notes

- GIVEN an item already has 2 notes
- WHEN the user adds a third note
- THEN the item now has 3 notes displayed

### Requirement: Edit Note

The system MUST allow users to edit the content of an existing note.

#### Scenario: Edit note content

- GIVEN a note with content "Revisar fecha"
- WHEN the user changes it to "Revisar fecha — probablemente 1811"
- THEN the note content is updated in the database
- AND the updated content is reflected in the UI

### Requirement: Delete Note

The system MUST allow users to delete a note. No confirmation is required for single note deletion.

#### Scenario: Delete a note

- GIVEN an item has 3 notes
- WHEN the user deletes one note
- THEN the note is removed from the database
- AND only 2 notes remain in the list

### Requirement: Note Display Order

Notes MUST be displayed in reverse chronological order (newest first).

#### Scenario: Newest note appears first

- GIVEN an item has notes created at 10:00, 11:00, and 12:00
- WHEN the user views the item's notes
- THEN the 12:00 note appears first and the 10:00 note appears last
