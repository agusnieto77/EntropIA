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

  it('renders only PLACE entities in a place group', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'p1', entityType: 'place', value: 'río de la Plata' }),
      makeEntity({ id: 'p2', entityType: 'place', value: 'ciudad de Córdoba' }),
    ]
    render(EntityViewer, { props: { entities } })
    expect(screen.getByText('río de la Plata')).toBeInTheDocument()
    expect(screen.getByText('ciudad de Córdoba')).toBeInTheDocument()
    const groups = screen.getAllByTestId('entity-group')
    expect(groups).toHaveLength(1)
  })

  it('renders only DATE entities in a date group', () => {
    const entities: Entity[] = [makeEntity({ id: 'd1', entityType: 'date', value: '25/05/1810' })]
    render(EntityViewer, { props: { entities } })
    expect(screen.getByText('25/05/1810')).toBeInTheDocument()
    const groups = screen.getAllByTestId('entity-group')
    expect(groups).toHaveLength(1)
  })

  it('renders only INSTITUTION entities in an institution group', () => {
    const entities: Entity[] = [
      makeEntity({ id: 'i1', entityType: 'institution', value: 'Real Audiencia' }),
    ]
    render(EntityViewer, { props: { entities } })
    expect(screen.getByText('Real Audiencia')).toBeInTheDocument()
    const groups = screen.getAllByTestId('entity-group')
    expect(groups).toHaveLength(1)
  })

  it('shows confidence percentage on entity pill', () => {
    const entity = makeEntity({
      id: 'c1',
      entityType: 'person',
      value: 'Don Manuel',
      confidence: 0.85,
    })
    render(EntityViewer, { props: { entities: [entity] } })
    expect(screen.getByTestId('entity-confidence')).toHaveTextContent('85%')
  })

  it('shows 100% confidence when confidence is 1.0', () => {
    const entity = makeEntity({
      id: 'c2',
      entityType: 'person',
      value: 'Doña Juana',
      confidence: 1.0,
    })
    render(EntityViewer, { props: { entities: [entity] } })
    expect(screen.getByTestId('entity-confidence')).toHaveTextContent('100%')
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

  it('does not call onhighlight when entity has null offsets', async () => {
    const onhighlight = vi.fn()
    const entity = makeEntity({ startOffset: null, endOffset: null, value: 'Sin offset' })
    render(EntityViewer, { props: { entities: [entity], onhighlight } })

    const pill = screen.getByText('Sin offset')
    await fireEvent.click(pill)

    expect(onhighlight).not.toHaveBeenCalled()
  })
})
