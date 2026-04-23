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

export interface LayoutRegion {
  category: string
  bbox: { x: number; y: number; width: number; height: number }
  confidence: number
  reading_order: number
}

export type ViewerType = 'image' | 'pdf' | 'audio'

export interface DocumentViewerProps {
  path: string
  type: ViewerType
  assetUrl: string
  annotations?: ViewerAnnotation[]
  layoutRegions?: LayoutRegion[]
  selectedAnnotationId?: string | null
  annotationTool?: AnnotationTool
  annotationColor?: string
  onAnnotationsChange?: (annotations: ViewerAnnotation[]) => void
  onSelectedAnnotationIdChange?: (annotationId: string | null) => void
  onAnnotationToolChange?: (tool: AnnotationTool) => void
  onAnnotationColorChange?: (color: string) => void
}
