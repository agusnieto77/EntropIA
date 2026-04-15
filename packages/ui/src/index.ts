// Design tokens
export { colors, spacing, typography, radius, shadows } from './tokens/index'

// Components — Fase 0
export { Button } from './components/Button/index'
export type { ButtonProps, ButtonVariant, ButtonSize } from './components/Button/index'

export { Input } from './components/Input/index'
export type { InputProps, InputType } from './components/Input/index'

export { Card } from './components/Card/index'
export type { CardProps, CardPadding } from './components/Card/index'

// Components — Fase 1
export { CollectionCard } from './components/CollectionCard/index'
export type { CollectionCardProps } from './components/CollectionCard/index'

export { ItemCard } from './components/ItemCard/index'
export type { ItemCardProps } from './components/ItemCard/index'

export { DocumentViewer } from './components/DocumentViewer/index'
export type {
  DocumentViewerProps,
  ViewerAnnotation,
  AnnotationKind,
  AnnotationTool,
} from './components/DocumentViewer/index'

export { SearchBar } from './components/SearchBar/index'
export type { SearchBarProps } from './components/SearchBar/index'

export { MetadataEditor } from './components/MetadataEditor/index'
export type { MetadataEditorProps } from './components/MetadataEditor/index'

export { NoteEditor } from './components/NoteEditor/index'
export type { NoteEditorProps } from './components/NoteEditor/index'

// Components — Fase 3
export { EntityViewer } from './components/EntityViewer/index'
export type { Entity, EntityType, EntityViewerProps } from './components/EntityViewer/index'
