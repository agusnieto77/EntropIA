<script lang="ts">
  import { onMount, onDestroy } from 'svelte'

  let canvas: HTMLCanvasElement
  let animationId: number
  let ctx: CanvasRenderingContext2D

  interface Star {
    x: number
    y: number
    vx: number
    vy: number
    radius: number
    opacity: number
    pulse: number
    pulseSpeed: number
  }

  const STAR_COUNT = 90
  const MAX_DIST = 160
  const stars: Star[] = []

  function resize() {
    canvas.width = window.innerWidth
    canvas.height = window.innerHeight
  }

  function initStars() {
    stars.length = 0
    for (let i = 0; i < STAR_COUNT; i++) {
      stars.push({
        x: Math.random() * canvas.width,
        y: Math.random() * canvas.height,
        vx: (Math.random() - 0.5) * 0.28,
        vy: (Math.random() - 0.5) * 0.28,
        radius: Math.random() * 1.6 + 0.4,
        opacity: Math.random() * 0.5 + 0.3,
        pulse: Math.random() * Math.PI * 2,
        pulseSpeed: Math.random() * 0.018 + 0.008,
      })
    }
  }

  function draw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height)

    // update
    for (const s of stars) {
      s.x += s.vx
      s.y += s.vy
      s.pulse += s.pulseSpeed
      if (s.x < 0) s.x = canvas.width
      if (s.x > canvas.width) s.x = 0
      if (s.y < 0) s.y = canvas.height
      if (s.y > canvas.height) s.y = 0
    }

    // draw edges
    for (let i = 0; i < stars.length; i++) {
      for (let j = i + 1; j < stars.length; j++) {
        const dx = stars[i].x - stars[j].x
        const dy = stars[i].y - stars[j].y
        const dist = Math.sqrt(dx * dx + dy * dy)
        if (dist < MAX_DIST) {
          const alpha = (1 - dist / MAX_DIST) * 0.18
          ctx.beginPath()
          ctx.moveTo(stars[i].x, stars[i].y)
          ctx.lineTo(stars[j].x, stars[j].y)
          ctx.strokeStyle = `rgba(147, 112, 219, ${alpha})`
          ctx.lineWidth = 0.7
          ctx.stroke()
        }
      }
    }

    // draw nodes
    for (const s of stars) {
      const pulse = 0.75 + 0.25 * Math.sin(s.pulse)
      const r = s.radius * pulse
      const alpha = s.opacity * pulse

      // glow
      const grd = ctx.createRadialGradient(s.x, s.y, 0, s.x, s.y, r * 4)
      grd.addColorStop(0, `rgba(180, 140, 255, ${alpha * 0.55})`)
      grd.addColorStop(1, `rgba(147, 112, 219, 0)`)
      ctx.beginPath()
      ctx.arc(s.x, s.y, r * 4, 0, Math.PI * 2)
      ctx.fillStyle = grd
      ctx.fill()

      // core
      ctx.beginPath()
      ctx.arc(s.x, s.y, r, 0, Math.PI * 2)
      ctx.fillStyle = `rgba(210, 180, 255, ${alpha})`
      ctx.fill()
    }

    animationId = requestAnimationFrame(draw)
  }

  onMount(() => {
    ctx = canvas.getContext('2d')!
    resize()
    initStars()
    draw()
    window.addEventListener('resize', () => { resize(); initStars() })
  })

  onDestroy(() => {
    cancelAnimationFrame(animationId)
    window.removeEventListener('resize', () => {})
  })
</script>

<canvas bind:this={canvas} class="constellation-bg" aria-hidden="true"></canvas>

<style>
  .constellation-bg {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 0;
    opacity: 0.65;
    /* Desenfoque suave para efecto difuso */
    filter: blur(0.6px);
  }
</style>
