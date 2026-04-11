export interface NoteEditorProps {
  content?: string
  placeholder?: string
  onsave?: (content: string) => void
  oncancel?: () => void
}
