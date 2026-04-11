export type EntityType = 'person' | 'place' | 'date' | 'institution' | 'custom'

export interface Entity {
  id: string
  itemId: string
  entityType: EntityType
  value: string
  startOffset: number | null
  endOffset: number | null
  confidence: number | null
  createdAt: number
}

export interface EntityViewerProps {
  entities: Entity[]
}

/** Display label per entity type */
export const ENTITY_TYPE_LABELS: Record<EntityType, string> = {
  person: 'Person',
  place: 'Place',
  date: 'Date',
  institution: 'Institution',
  custom: 'Custom',
}

/** CSS class suffix per entity type for color-coding */
export const ENTITY_TYPE_COLORS: Record<EntityType, string> = {
  person: 'person',
  place: 'place',
  date: 'date',
  institution: 'institution',
  custom: 'custom',
}
