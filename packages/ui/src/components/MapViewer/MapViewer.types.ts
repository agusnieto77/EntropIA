export interface MapMarker {
  entityId: string
  label: string
  latitude: number
  longitude: number
  itemId?: string
  itemTitle?: string
}

export interface MapViewerProps {
  markers: MapMarker[]
  height?: string
  onmarkerclick?: (marker: MapMarker) => void
}
