/**
 * Navigation store for the desktop app.
 * Uses plain TypeScript (no Svelte runes) for testability in Node/Vitest.
 * Svelte components importing this will still react via their own reactivity.
 */

export type View =
  | { name: 'collections' }
  | { name: 'collection'; id: string; collectionName: string }
  | {
      name: 'item'
      collectionId: string
      collectionName: string
      itemId: string
      itemTitle: string
    }

export class NavigationStore {
  private _history: View[] = [{ name: 'collections' }]

  get current(): View {
    return this._history.at(-1)!
  }

  get canGoBack(): boolean {
    return this._history.length > 1
  }

  get breadcrumb(): string[] {
    return this._history.map((v) => {
      if (v.name === 'collections') return 'Collections'
      if (v.name === 'collection') return v.collectionName
      return v.itemTitle
    })
  }

  navigate(view: View): void {
    this._history = [...this._history, view]
  }

  back(): void {
    if (this._history.length > 1) {
      this._history = this._history.slice(0, -1)
    }
  }
}

export const navigation = new NavigationStore()
