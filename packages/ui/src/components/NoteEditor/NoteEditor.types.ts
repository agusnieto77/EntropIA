export interface NoteEditorProps {
  content?: string
  placeholder?: string
  onsave?: (content: string) => void | Promise<void>
  oncancel?: () => void
  clearOnSave?: boolean
  saveLabel?: string
  cancelLabel?: string
}
