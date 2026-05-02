export interface NoteEditorProps {
  content?: string
  placeholder?: string
  onsave?: (content: string) => void | Promise<void>
  oncancel?: () => void
  ondictate?: (audio: Blob) => Promise<string>
  dictationMaxSeconds?: number
  clearOnSave?: boolean
  saveLabel?: string
  cancelLabel?: string
}
