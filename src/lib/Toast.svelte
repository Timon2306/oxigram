<script lang="ts">
  import { toastState } from "./notify.svelte";
</script>

{#if toastState.text}
  <div class="toast" role="status">{toastState.text}</div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 28px;
    z-index: 40;
    max-width: min(480px, calc(100% - 32px));
    transform: translateX(-50%);
    padding: 9px 16px;
    border-radius: 18px;
    background: #1f2c3a;
    box-shadow: 0 6px 24px rgb(0 0 0 / 45%);
    color: var(--text);
    font-size: 14px;
    pointer-events: none;
  }
</style>
