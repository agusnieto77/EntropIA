<script lang="ts">
  import { onMount } from 'svelte'

  interface EntropicNode {
    id: number
    x: number
    y: number
    size: number
    alpha: number
    depth: number
  }

  const MIN_NODES = 420
  const MAX_NODES = 1200
  const NODE_DENSITY = 2200
  const LINK_DISTANCE = 118
  const GRID_SIZE = LINK_DISTANCE
  const MAX_DEVICE_PIXEL_RATIO = 1.35
  const CANVAS_OVERSCAN = 140

  interface ThemePalette {
    bg: [string, string, string]
    haze: [string, string, string]
    node: string
    link: string
  }

  const PALETTES: Record<string, ThemePalette> = {
    dark: {
      bg: ['#080a10', '#0c0f17', '#10131b'],
      haze: ['rgba(75, 83, 106, 0.08)', 'rgba(38, 44, 62, 0.045)', 'rgba(8, 10, 16, 0)'],
      node: 'rgba(190, 197, 214, {a})',
      link: 'rgba(146, 154, 178, {a})',
    },
    dim: {
      bg: ['#15130f', '#1a1813', '#1d1a15'],
      haze: ['rgba(106, 90, 60, 0.08)', 'rgba(62, 52, 38, 0.045)', 'rgba(21, 19, 15, 0)'],
      node: 'rgba(200, 185, 160, {a})',
      link: 'rgba(170, 155, 130, {a})',
    },
    light: {
      bg: ['#f8f9fc', '#f0f1f6', '#eaecf3'],
      haze: ['rgba(140, 150, 180, 0.06)', 'rgba(100, 110, 140, 0.03)', 'rgba(248, 249, 252, 0)'],
      node: 'rgba(80, 90, 120, {a})',
      link: 'rgba(100, 110, 140, {a})',
    },
  }

  function getCurrentPalette(): ThemePalette {
    const theme = document.documentElement.dataset.theme ?? 'dark'
    return PALETTES[theme] ?? PALETTES.dark
  }

  let canvas: HTMLCanvasElement
  let ctx: CanvasRenderingContext2D | null = null
  let nodes: EntropicNode[] = []
  let width = 0
  let height = 0
  let deviceScale = 1
  let reducedMotion = false
  let resizeTimer: ReturnType<typeof setTimeout> | null = null
  let themeObserver: MutationObserver | null = null

  function clamp(value: number, min: number, max: number) {
    return Math.min(max, Math.max(min, value))
  }

  function randomBetween(min: number, max: number) {
    return min + Math.random() * (max - min)
  }

  function generateNodes(nextWidth: number, nextHeight: number): EntropicNode[] {
    const area = nextWidth * nextHeight
    const count = clamp(Math.floor(area / NODE_DENSITY), MIN_NODES, MAX_NODES)
    const anchorCount = clamp(Math.floor(nextWidth / 210), 7, 18)
    const anchors = Array.from({ length: anchorCount }, (_, index) => {
      const normalized = anchorCount === 1 ? 0.5 : index / (anchorCount - 1)
      return {
        x: normalized * nextWidth + randomBetween(-nextWidth * 0.06, nextWidth * 0.06),
        y: nextHeight * randomBetween(0.12, 0.9),
      }
    })

    return Array.from({ length: count }, (_, index) => {
      const usesAnchor = Math.random() > 0.18
      const anchor = anchors[index % anchors.length] ?? { x: nextWidth / 2, y: nextHeight / 2 }
      const x = usesAnchor
        ? clamp(anchor.x + randomBetween(-nextWidth * 0.13, nextWidth * 0.13), 0, nextWidth)
        : Math.random() * nextWidth
      const y = usesAnchor
        ? clamp(anchor.y + randomBetween(-nextHeight * 0.18, nextHeight * 0.18), 0, nextHeight)
        : Math.random() * nextHeight
      const depth = Math.random()

      return {
        id: index,
        x,
        y,
        size: randomBetween(0.35, 1.1) + depth * 0.42,
        alpha: randomBetween(0.045, 0.18) + depth * 0.07,
        depth,
      }
    })
  }

  function resizeCanvas() {
    if (!canvas) return

    width = Math.max(1, window.innerWidth + CANVAS_OVERSCAN * 2)
    height = Math.max(1, window.innerHeight + CANVAS_OVERSCAN * 2)
    deviceScale = Math.min(window.devicePixelRatio || 1, MAX_DEVICE_PIXEL_RATIO)

    canvas.width = Math.floor(width * deviceScale)
    canvas.height = Math.floor(height * deviceScale)
    canvas.style.width = `${width}px`
    canvas.style.height = `${height}px`
    canvas.style.left = `${-CANVAS_OVERSCAN}px`
    canvas.style.top = `${-CANVAS_OVERSCAN}px`

    ctx = canvas.getContext('2d', { alpha: false })
    ctx?.setTransform(deviceScale, 0, 0, deviceScale, 0, 0)
    nodes = generateNodes(width, height)
    renderConstellation()
  }

  function scheduleResize() {
    if (resizeTimer) clearTimeout(resizeTimer)
    resizeTimer = setTimeout(resizeCanvas, 120)
  }

  function drawBackground(context: CanvasRenderingContext2D, palette: ThemePalette) {
    const gradient = context.createLinearGradient(0, 0, width, height)
    gradient.addColorStop(0, palette.bg[0])
    gradient.addColorStop(0.46, palette.bg[1])
    gradient.addColorStop(1, palette.bg[2])
    context.fillStyle = gradient
    context.fillRect(0, 0, width, height)

    const haze = context.createRadialGradient(width * 0.5, height * 0.52, 0, width * 0.5, height * 0.52, width * 0.72)
    haze.addColorStop(0, palette.haze[0])
    haze.addColorStop(0.48, palette.haze[1])
    haze.addColorStop(1, palette.haze[2])
    context.fillStyle = haze
    context.fillRect(0, 0, width, height)
  }

  function buildSpatialGrid() {
    const grid = new Map<string, EntropicNode[]>()
    for (const node of nodes) {
      const cellX = Math.floor(node.x / GRID_SIZE)
      const cellY = Math.floor(node.y / GRID_SIZE)
      const key = `${cellX}:${cellY}`
      const bucket = grid.get(key)
      if (bucket) bucket.push(node)
      else grid.set(key, [node])
    }
    return grid
  }

  function drawLinks(context: CanvasRenderingContext2D, grid: Map<string, EntropicNode[]>, palette: ThemePalette) {
    context.lineWidth = 0.45

    for (const [key, bucket] of grid) {
      const [rawX, rawY] = key.split(':')
      const cellX = Number(rawX)
      const cellY = Number(rawY)

      for (let dx = -1; dx <= 1; dx++) {
        for (let dy = -1; dy <= 1; dy++) {
          const neighbor = grid.get(`${cellX + dx}:${cellY + dy}`)
          if (!neighbor) continue

          for (const a of bucket) {
            for (const b of neighbor) {
              if (b.id <= a.id) continue
              const diffX = a.x - b.x
              const diffY = a.y - b.y
              const distance = Math.hypot(diffX, diffY)
              if (distance > LINK_DISTANCE) continue

              const alpha = (1 - distance / LINK_DISTANCE) * 0.043 * (0.75 + (a.depth + b.depth) * 0.34)
              context.beginPath()
              context.moveTo(a.x, a.y)
              context.lineTo(b.x, b.y)
              context.strokeStyle = palette.link.replace('{a}', String(alpha))
              context.stroke()
            }
          }
        }
      }
    }
  }

  function drawNodes(context: CanvasRenderingContext2D, palette: ThemePalette) {
    for (const node of nodes) {
      context.beginPath()
      context.arc(node.x, node.y, node.size, 0, Math.PI * 2)
      context.fillStyle = palette.node.replace('{a}', String(node.alpha))
      context.fill()
    }
  }

  function renderConstellation() {
    if (!ctx || !canvas) return

    const palette = getCurrentPalette()
    drawBackground(ctx, palette)
    const grid = buildSpatialGrid()
    drawLinks(ctx, grid, palette)
    drawNodes(ctx, palette)
  }

  onMount(() => {
    reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
    resizeCanvas()
    window.addEventListener('resize', scheduleResize)

    themeObserver = new MutationObserver(() => renderConstellation())
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    })

    return () => {
      if (resizeTimer) clearTimeout(resizeTimer)
      window.removeEventListener('resize', scheduleResize)
      themeObserver?.disconnect()
    }
  })
</script>

<canvas
  bind:this={canvas}
  class="constellation"
  class:constellation--motion={!reducedMotion}
  aria-hidden="true"
></canvas>

<style>
  .constellation {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 0;
    pointer-events: none;
    transform-origin: center;
    will-change: transform, opacity;
  }

  .constellation--motion {
    animation: entropic-drift 96s linear infinite alternate;
  }

  @keyframes entropic-drift {
    0% {
      opacity: 0.92;
      transform: translate3d(-6px, -2px, 0) rotate(-0.12deg) scale(1.006);
    }
    50% {
      opacity: 0.98;
      transform: translate3d(5px, 4px, 0) rotate(0.1deg) scale(1.01);
    }
    100% {
      opacity: 0.94;
      transform: translate3d(11px, -5px, 0) rotate(0.22deg) scale(1.008);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .constellation--motion {
      animation: none;
    }
  }
</style>
