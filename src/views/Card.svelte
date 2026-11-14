<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<section class="wrap">
  <div class="card">
    <h1>{title}</h1>
    {@render children()}
  </div>
</section>

<style>
  .wrap {
    min-height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .card {
    width: 100%;
    max-width: 380px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 24px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: 12px;
  }

  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
  }
</style>
