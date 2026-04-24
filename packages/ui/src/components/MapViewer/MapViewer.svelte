<script lang="ts">
  import type { MapViewerProps, MapMarker } from './MapViewer.types'
  import { onMount, onDestroy } from 'svelte'
  import L from 'leaflet'
  import 'leaflet/dist/leaflet.css'

  let { markers = [], height = '300px', onmarkerclick }: MapViewerProps = $props()

  let mapContainer: HTMLDivElement | undefined = $state()
  let map: L.Map | null = null
  let markerLayer: L.LayerGroup | null = null

  const defaultCenter: L.LatLngExpression = [-34.6, -58.4] // Buenos Aires default
  const defaultZoom = 3

  onMount(() => {
    if (!mapContainer) return

    map = L.map(mapContainer).setView(defaultCenter, defaultZoom)

    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>',
      maxZoom: 19,
    }).addTo(map)

    markerLayer = L.layerGroup().addTo(map)
    updateMarkers()
  })

  onDestroy(() => {
    if (map) {
      map.remove()
      map = null
    }
  })

  function updateMarkers() {
    if (!map || !markerLayer) return

    markerLayer.clearLayers()

    const bounds: L.LatLng[] = []

    for (const m of markers) {
      const latLng = L.latLng(m.latitude, m.longitude)
      bounds.push(latLng)

      const leafletMarker = L.marker(latLng).addTo(markerLayer)

      const popupContent = m.itemTitle
        ? `<strong>${m.label}</strong><br><em>${m.itemTitle}</em>`
        : `<strong>${m.label}</strong>`
      leafletMarker.bindPopup(popupContent)

      if (onmarkerclick) {
        leafletMarker.on('click', () => onmarkerclick(m))
      }
    }

    if (bounds.length > 0) {
      const group = L.latLngBounds(bounds)
      map.fitBounds(group, { padding: [30, 30], maxZoom: 12 })
    }
  }

  $effect(() => {
    void markers
    updateMarkers()
  })
</script>

<div class="map-viewer" style="height: {height}">
  <div class="map-viewer__container" bind:this={mapContainer}></div>
  {#if markers.length === 0}
    <div class="map-viewer__empty">
      <p>No hay ubicaciones georreferenciadas</p>
    </div>
  {/if}
</div>

<style>
  .map-viewer {
    position: relative;
    border: 1px solid var(--color-border, #e2e8f0);
    border-radius: var(--radius-sm, 4px);
    overflow: hidden;
  }

  .map-viewer__container {
    width: 100%;
    height: 100%;
  }

  .map-viewer__empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-surface, #f8fafc);
    pointer-events: none;
  }

  .map-viewer__empty p {
    color: var(--color-text-muted, #94a3b8);
    font-size: var(--font-size-sm, 0.875rem);
  }
</style>
