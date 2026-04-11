import { describe, it, expect, beforeEach } from 'vitest'
import { NavigationStore, type View } from './navigation'

describe('NavigationStore', () => {
  let nav: NavigationStore

  beforeEach(() => {
    nav = new NavigationStore()
  })

  it('starts at collections view', () => {
    expect(nav.current).toEqual({ name: 'collections' })
  })

  it('canGoBack is false at root', () => {
    expect(nav.canGoBack).toBe(false)
  })

  it('navigate adds view to history and updates current', () => {
    const view: View = { name: 'collection', id: 'c1', collectionName: 'My Collection' }
    nav.navigate(view)
    expect(nav.current).toEqual(view)
    expect(nav.canGoBack).toBe(true)
  })

  it('navigate to item view shows item as current', () => {
    const collectionView: View = { name: 'collection', id: 'c1', collectionName: 'Coll A' }
    const itemView: View = {
      name: 'item',
      collectionId: 'c1',
      collectionName: 'Coll A',
      itemId: 'i1',
      itemTitle: 'Document 1',
    }
    nav.navigate(collectionView)
    nav.navigate(itemView)
    expect(nav.current).toEqual(itemView)
  })

  it('back removes last view and updates current', () => {
    nav.navigate({ name: 'collection', id: 'c1', collectionName: 'Test' })
    nav.back()
    expect(nav.current).toEqual({ name: 'collections' })
    expect(nav.canGoBack).toBe(false)
  })

  it('back is no-op at root', () => {
    nav.back()
    expect(nav.current).toEqual({ name: 'collections' })
    expect(nav.canGoBack).toBe(false)
  })

  it('back traverses full history correctly', () => {
    nav.navigate({ name: 'collection', id: 'c1', collectionName: 'A' })
    nav.navigate({
      name: 'item',
      collectionId: 'c1',
      collectionName: 'A',
      itemId: 'i1',
      itemTitle: 'Doc',
    })
    nav.back()
    expect(nav.current).toEqual({ name: 'collection', id: 'c1', collectionName: 'A' })
    nav.back()
    expect(nav.current).toEqual({ name: 'collections' })
  })

  it('breadcrumb builds from history', () => {
    expect(nav.breadcrumb).toEqual(['Collections'])

    nav.navigate({ name: 'collection', id: 'c1', collectionName: 'Photos' })
    expect(nav.breadcrumb).toEqual(['Collections', 'Photos'])

    nav.navigate({
      name: 'item',
      collectionId: 'c1',
      collectionName: 'Photos',
      itemId: 'i1',
      itemTitle: 'Sunset.jpg',
    })
    expect(nav.breadcrumb).toEqual(['Collections', 'Photos', 'Sunset.jpg'])
  })

  it('breadcrumb updates after back', () => {
    nav.navigate({ name: 'collection', id: 'c1', collectionName: 'Docs' })
    nav.navigate({
      name: 'item',
      collectionId: 'c1',
      collectionName: 'Docs',
      itemId: 'i1',
      itemTitle: 'Report',
    })
    nav.back()
    expect(nav.breadcrumb).toEqual(['Collections', 'Docs'])
  })
})
