<script lang="ts">
  interface Props {
    id?: string;
    value: string;
    onchange: (value: string) => void;
    disabled?: boolean;
  }

  let { id, value, onchange, disabled = false }: Props = $props();
</script>

<select
  {id}
  class="select"
  {value}
  onchange={(e) => onchange(e.currentTarget.value)}
  {disabled}
>
  <slot />
</select>

<style>
  .select {
    padding: 0.5rem 0.75rem;
    border: 2px solid #ddd;
    border-radius: 6px;
    background: #fff;
    font-size: 0.875rem;
    cursor: pointer;
  }

  .select:focus {
    outline: none;
    border-color: #007aff;
  }

  .select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  @media (prefers-color-scheme: dark) {
    .select {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .select option {
      background: #333;
      color: #eee;
    }

    .select:focus {
      border-color: #0a84ff;
    }
  }
</style>
