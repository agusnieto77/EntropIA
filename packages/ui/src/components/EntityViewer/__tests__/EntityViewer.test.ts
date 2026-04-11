import { render, screen, fireEvent } from '@testing-library/svelte'
import { describe, it, expect, vi } from 'vitest'
import EntityViewer from '../EntityViewer.svelte'
import type { Entity } from '../EntityViewer.types'

const makeEntity = (overrides: Partial<Entity> = {}): Entity => ({
  id: 'ent-1',
  itemId: 'item-1',
  entityType: 'person',
  value: 'Don Manuel Belgrano',
  startOffset: 0,
  endOffset: 19,
  confidence: 1.0,
  createdAt: 1700000000000,
  ...overrides,
})

describe('EntityViewer', () => {
  // ─────────────────────────────────────────────────────────────────────────
  // Empty state
  // ─────────────────────────────────────────────────────────────────────────

  it('shows empty state message when entities array is empty', () => {
    render(EntityViewer, { props: { entities: [] } })
    expect(screen.getByTestId('entity-viewer-empty')).toBeInTheDocument()
  })

  it('does not render any group sections when entities is empty', () => {
    render(EntityViewer, { props: { entities: [] } })
    expect(screen.queryByTestId('entity-group')).not.toBeInTheDocument()
  })

  // ─────────────────────────────────────────────────────────────────────────
  // Grouped rendering
  // ─────────────────────────────────────────────────────────────────────────

  it('renders 3 group sections for 2 PERSON + 1 PLACE + 1 DATE', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'e1', entityType: 'person', value: 'Don Manuel Belgrano' }),
      makeEntity({ id: 'e2', entityType: 'person', value: 'Doña Juana Azurduy' }),
      makeEntity({ id: 'e3', entityType: 'place', value: 'ciudad de Buenos Aires' }),
      makeEntity({ id: 'e4', entityType: 'date', value: '15 de mayo de 1810' }),
    ]
    render(EntityViewer, { props: { entities } })
    const groups = screen.getAllByTestId('entity-group')
    expect(groups).toHaveLength(3)
  })

  it('renders group labels for detected entity types', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'e1', entityType: 'person', value: 'Fray Bartolomé' }),
      makeEntity({ id: 'e2', entityType: 'institution', value: 'Cabildo' }),
    ]
    render(EntityViewer, { props: { entities } })
    expect(screen.getByText(/person/i)).toBeInTheDocument()
    expect(screen.getByText(/institution/i)).toBeInTheDocument()
  })

  it('renders all entity values as pills', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'e1', entityType: 'person', value: 'Don Manuel Belgrano' }),
      makeEntity({ id: 'e2', entityType: 'place', value: 'ciudad de Buenos Aires' }),
    ]
    render(EntityViewer, { props: { entities } })
    expect(screen.getByText('Don Manuel Belgrano')).toBeInTheDocument()
    expect(screen.getByText('ciudad de Buenos Aires')).toBeInTheDocument()
  })

  it('renders all 4 entity type groups when all types present', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'e1', entityType: 'person', value: 'Don Pedro' }),
      makeEntity({ id: 'e2', entityType: 'place', value: 'villa de Potosí' }),
      makeEntity({ id: 'e3', entityType: 'date', value: '12 de octubre de 1492' }),
      makeEntity({ id: 'e4', entityType: 'institution', value: 'Real Audiencia' }),
    ]
    render(EntityViewer, { props: { entities } })
    const groups = screen.getAllByTestId('entity-group')
    expect(groups).toHaveLength(4)
  })

  // ─────────────────────────────────────────────────────────────────────────
  // Click dispatches highlight event
  // ─────────────────────────────────────────────────────────────────────────

  it('clicking an entity pill calls onhighlight with correct offsets', async () => {
    const onhighlight = vi.fn()
    const entity = makeEntity({ startOffset: 10, endOffset: 30, value: 'Don Manuel Belgrano' })
    render(EntityViewer, { props: { entities: [entity], onhighlight } })

    const pill = screen.getByText('Don Manuel Belgrano')
    await fireEvent.click(pill)

    expect(onhighlight).toHaveBeenCalledOnce()
    expect(onhighlight).toHaveBeenCalledWith({ startOffset: 10, endOffset: 30 })
  })
})
