<script lang="ts">
  interface Props {
    label: string;
    value: string;
  }

  let { label, value }: Props = $props();

  function copy() {
    navigator.clipboard.writeText(value);
  }
</script>

<div class="connection-info">
  <span class="connection-label">{label}</span>
  <div class="url-row">
    <code class="connection-address">{value}</code>
    <button class="copy-btn" onclick={copy}>Copy</button>
  </div>
</div>

<style>
  .connection-info {
    background: #e8f4fd;
    border: 1px solid #b3d9f7;
    border-radius: 8px;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .connection-label {
    font-size: 0.75rem;
    color: #1a73e8;
    font-weight: 500;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .connection-address {
    font-size: 1.125rem;
    font-weight: 600;
    color: #1a56c4;
  }

  .copy-btn {
    padding: 0.25rem 0.5rem;
    font-size: 0.7rem;
    border: 1px solid #b3d9f7;
    border-radius: 4px;
    background: #fff;
    color: #1a73e8;
    cursor: pointer;
  }

  .copy-btn:hover {
    background: #d0e8fc;
  }

  @media (prefers-color-scheme: dark) {
    .connection-info {
      background: #1a3a5c;
      border-color: #2a5a8c;
    }

    .connection-label {
      color: #6ab7ff;
    }

    .connection-address {
      color: #8fcfff;
    }

    .copy-btn {
      background: #2a5a8c;
      border-color: #3a6a9c;
      color: #8fcfff;
    }

    .copy-btn:hover {
      background: #3a6a9c;
    }
  }
</style>
