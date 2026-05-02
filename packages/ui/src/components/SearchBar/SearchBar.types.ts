export interface SearchBarProps {
  value?: string
  placeholder?: string
  debounceMs?: number
  ariaLabel?: string
  clearAriaLabel?: string
  onsearch?: (query: string) => void
  onclear?: () => void
}
