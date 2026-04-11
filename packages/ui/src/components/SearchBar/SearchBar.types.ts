export interface SearchBarProps {
  value?: string
  placeholder?: string
  debounceMs?: number
  onsearch?: (query: string) => void
  onclear?: () => void
}
