<script lang="ts">
  import { onMount } from 'svelte'

  interface Star {
    x: number
    y: number
    size: number
    opacity: number
    twinkleSpeed: number
    twinklePhase: number
  }

  let canvas: HTMLCanvasElement
  let stars: Star[] = []
  let animationId: number

  function initStars(width: number, height: number): Star[] {
    const count = Math.floor((width * height) / 8000)
    return Array.from({ length: count }, () => ({
      x: Math.random() * width,
      y: Math.random() * height,
      size: Math.random() * 2 + 0.5,
      opacity: Math.random() * 0.6 + 0.2,
      twinkleSpeed: Math.random() * 0.02 + 0.005,
      twinklePhase: Math.random() * Math.PI * 2,
    }))
  }

  function animate() {
    if (!canvas) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return

    ctx.clearRect(0, 0, canvas.width, canvas.height)

    const time = Date.now() * 0.001

    for (const star of stars) {
      const twinkle = Math.sin(time * star.twinkleSpeed + star.twinklePhase)
      const alpha = star.opacity * (0.5 + twinkle * 0.5)

      ctx.beginPath()
      ctx.arc(star.x, star.y, star.size, 0, Math.PI * 2)
      ctx.fillStyle = `rgba(200, 210, 230, ${alpha})`
      ctx.fill()

      if (star.size > 1.2) {
        const gradient = ctx.createRadialGradient(
          star.x,
          star.y,
          0,
          star.x,
          star.y,
          star.size * 3
        )
        gradient.addColorStop(0, `rgba(180, 190, 220, ${alpha * 0.3})`)
        gradient.addColorStop(1, 'transparent')
        ctx.beginPath()
        ctx.arc(star.x, star.y, star.size * 3, 0, Math.PI * 2)
        ctx.fillStyle = gradient
        ctx.fill()
      }
    }

    for (let i = 0; i < stars.length; i++) {
      for (let j = i + 1; j < stars.length; j++) {
        const dx = stars[i].x - stars[j].x
        const dy = stars[i].y - stars[j].y
        const dist = Math.sqrt(dx * dx + dy * dy)

        if (dist < 120) {
          const lineOpacity = (1 - dist / 120) * 0.08
          ctx.beginPath()
          ctx.moveTo(stars[i].x, stars[i].y)
          ctx.lineTo(stars[j].x, stars[j].y)
          ctx.strokeStyle = `rgba(150, 160, 200, ${lineOpacity})`
          ctx.lineWidth = 0.5
          ctx.stroke()
        }
      }
    }

    animationId = requestAnimationFrame(animate)
  }

  function resize() {
    if (!canvas) return
    canvas.width = window.innerWidth
    canvas.height = window.innerHeight
    stars = initStars(canvas.width, canvas.height)
  }

  onMount(() => {
    resize()
    window.addEventListener('resize', resize)
    animate()

    return () => {
      window.removeEventListener('resize', resize)
      cancelAnimationFrame(animationId)
    }
  })
</script>

<canvas bind:this={canvas} class="constellation"></canvas>

<style>
  .constellation {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    z-index: -1;
    pointer-events: none;
    background: linear-gradient(
      135deg,
      rgba(15, 20, 35, 0.92) 0%,
      rgba(20, 25, 40, 0.88) 50%,
      rgba(25, 30, 45, 0.85) 100%
    );
  }
</style>