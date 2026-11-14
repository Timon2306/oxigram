<script lang="ts">
  /**
   * A loading ring with the percentage inside, or a spinning arc when the
   * total size is unknown. Cheap: one SVG, one CSS animation while shown.
   */
  let {
    done = 0,
    total = 0,
    size = 72,
  }: {
    /** Bytes received so far. */
    done?: number;
    /** Total bytes; 0 when unknown. */
    total?: number;
    size?: number;
  } = $props();

  const RADIUS = 30;
  const LENGTH = 2 * Math.PI * RADIUS;

  const percent = $derived(total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : null);
</script>

<div class="progress" style:width="{size}px" style:height="{size}px" role="progressbar" aria-valuenow={percent ?? undefined} aria-valuemin={0} aria-valuemax={100}>
  <svg viewBox="0 0 72 72" class:spin={percent === null}>
    <circle class="track" cx="36" cy="36" r={RADIUS} />
    <circle
      class="bar"
      cx="36"
      cy="36"
      r={RADIUS}
      stroke-dasharray={LENGTH}
      stroke-dashoffset={percent === null ? LENGTH * 0.72 : LENGTH * (1 - percent / 100)}
    />
  </svg>
  {#if percent !== null}<span>{percent}%</span>{/if}
</div>

<style>
  .progress {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: rgb(0 0 0 / 45%);
  }

  svg {
    position: absolute;
    inset: 0;
    transform: rotate(-90deg);
  }

  svg.spin {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(-90deg);
    }
    to {
      transform: rotate(270deg);
    }
  }

  circle {
    fill: none;
    stroke-width: 4;
  }

  .track {
    stroke: rgb(255 255 255 / 18%);
  }

  .bar {
    stroke: #fff;
    stroke-linecap: round;
    transition: stroke-dashoffset 0.2s;
  }

  span {
    position: relative;
    color: #fff;
    font-size: 15px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
