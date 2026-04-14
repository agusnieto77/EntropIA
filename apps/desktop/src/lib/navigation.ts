/**
 * Navigation store for the desktop app.
 * Exposes imperative API plus a lightweight subscription mechanism
 * so Svelte components can react to navigation changes.
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

type NavigationSnapshot = {
  history: View[]
  current: View
  canGoBack: boolean
  breadcrumb: string[]
}

type NavigationSubscriber = (snapshot: NavigationSnapshot) => void

export class NavigationStore {
  private _history: View[] = [{ name: 'collections' }]
  private readonly _subscribers = new Set<NavigationSubscriber>()

  subscribe(run: NavigationSubscriber): () => void {
    this._subscribers.add(run)
    run(this.snapshot())
    return () => {
      this._subscribers.delete(run)
    }
  }

  private snapshot(): NavigationSnapshot {
    const history = [...this._history]
    return {
      history,
      current: history.at(-1)!,
      canGoBack: history.length > 1,
      breadcrumb: history.map((v) => {
        if (v.name === 'collections') return 'Collections'
        if (v.name === 'collection') return v.collectionName
        return v.itemTitle
      }),
    }
  }

  private emit(): void {
    const snapshot = this.snapshot()
    this._subscribers.forEach((run) => run(snapshot))
  }

  get current(): View {
    return this._history.at(-1)!
  }

  get canGoBack(): boolean {
    return this._history.length > 1
  }

  get breadcrumb(): string[] {
    return this.snapshot().breadcrumb
  }

  navigate(view: View): void {
    this._history = [...this._history, view]
    this.emit()
  }

  back(): void {
    if (this._history.length > 1) {
      this._history = this._history.slice(0, -1)
      this.emit()
    }
  }
}

export const navigation = new NavigationStore()
