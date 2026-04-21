export type EntityType =
  | 'person'
  | 'place'
  | 'date'
  | 'institution'
  | 'organization'
  | 'misc'
  | 'custom'

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
  organization: 'Organization',
  misc: 'Misc',
  custom: 'Custom',
}

export const ENTITY_TYPE_TAGS: Record<EntityType, string> = {
  person: 'PER',
  place: 'LOC',
  date: 'DATE',
  institution: 'ORG',
  organization: 'ORG',
  misc: 'MISC',
  custom: 'CUSTOM',
}

/** CSS class suffix per entity type for color-coding */
export const ENTITY_TYPE_COLORS: Record<EntityType, string> = {
  person: 'person',
  place: 'place',
  date: 'date',
  institution: 'institution',
  organization: 'organization',
  misc: 'misc',
  custom: 'custom',
}
