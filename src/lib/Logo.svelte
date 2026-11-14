<script lang="ts">
  /** The Oxigram mark, same artwork as the app icon. */
  let { size = 96 }: { size?: number } = $props();
</script>

<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width={size} height={size} aria-hidden="true">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#2fd2c6"/>
      <stop offset="0.55" stop-color="#2a8cf0"/>
      <stop offset="1" stop-color="#5a4ff0"/>
    </linearGradient>
    <linearGradient id="ring" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="1" stop-color="#dff1ff"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.3" cy="0.2" r="0.9">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.28"/>
      <stop offset="0.6" stop-color="#ffffff" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <rect x="32" y="32" width="960" height="960" rx="230" fill="url(#tile)"/>
  <rect x="32" y="32" width="960" height="960" rx="230" fill="url(#glow)"/>

  <path d="M672 330 A230 230 0 1 0 742 512" fill="none" stroke="url(#ring)" stroke-width="92" stroke-linecap="round"/>

  <g transform="translate(700 300)">
    <path d="M-92 -78h184a62 62 0 0 1 62 62v66a62 62 0 0 1-62 62h-70l-74 58 8-58h-48a62 62 0 0 1-62-62v-66a62 62 0 0 1 62-62z" fill="#ffffff"/>
    <circle cx="-52" cy="17" r="20" fill="#2a8cf0"/>
    <circle cx="8" cy="17" r="20" fill="#2a8cf0"/>
    <circle cx="68" cy="17" r="20" fill="#2a8cf0"/>
  </g>

  <circle cx="300" cy="790" r="30" fill="#ffffff" fill-opacity="0.55"/>
  <circle cx="226" cy="866" r="17" fill="#ffffff" fill-opacity="0.4"/>
</svg>
