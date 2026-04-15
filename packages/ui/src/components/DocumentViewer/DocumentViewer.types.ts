export type AnnotationKind = 'rectangle' | 'underline'
export type AnnotationTool = 'select' | AnnotationKind

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

export interface DocumentViewerProps {
  path: string
  type: 'image' | 'pdf'
  assetUrl: string
  annotations?: ViewerAnnotation[]
  selectedAnnotationId?: string | null
  annotationTool?: AnnotationTool
  annotationColor?: string
  onAnnotationsChange?: (annotations: ViewerAnnotation[]) => void
  onSelectedAnnotationIdChange?: (annotationId: string | null) => void
  onAnnotationToolChange?: (tool: AnnotationTool) => void
  onAnnotationColorChange?: (color: string) => void
}
