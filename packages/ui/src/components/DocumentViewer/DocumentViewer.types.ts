export type AnnotationKind = 'rectangle' | 'underline'
export type AnnotationTool = 'select' | AnnotationKind

export type EditTool = 'none' | 'crop' | 'erase'

export interface ImageEditResult {
  path: string
  width: number
  height: number
  format_changed: boolean
  /** Path of the file before the edit (kept on disk for undo) */
  previous_path: string
}

export interface ViewerAnnotation {
  id: string
  assetId: string
  page: number
  kind: AnnotationKind
  color: string
  x: number
  y: number
  width: number
  height: number
  createdAt: number
  updatedAt: number
}

export type ViewerType = 'image' | 'pdf' | 'audio'

export interface DocumentViewerProps {
  path: string
  type: ViewerType
  assetUrl: string
  annotations?: ViewerAnnotation[]
  selectedAnnotationId?: string | null
  annotationTool?: AnnotationTool
  annotationColor?: string
  editTool?: EditTool
  canUndo?: boolean
  onAnnotationsChange?: (annotations: ViewerAnnotation[]) => void
  onSelectedAnnotationIdChange?: (annotationId: string | null) => void
  onAnnotationToolChange?: (tool: AnnotationTool) => void
  onAnnotationColorChange?: (color: string) => void
  onEditSelect?: (region: { x: number; y: number; width: number; height: number }) => void
  onEditToolChange?: (tool: EditTool) => void
  onRotateLeft?: () => void
  onRotateRight?: () => void
  onUndo?: () => void
  onDimensionsChange?: (dimensions: { width: number; height: number }) => void
}