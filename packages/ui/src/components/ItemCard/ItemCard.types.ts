export interface ItemCardProps {
  id: string
  title: string
  assetCount: number
  thumbnailPath?: string
  metadataPreview?: string
  onclick?: () => void
}
