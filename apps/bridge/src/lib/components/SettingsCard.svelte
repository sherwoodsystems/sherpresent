<script lang="ts">
  import type { BridgeConfig } from '$lib/types';

  let {
    draft,
    onSave,
    onReset,
  }: {
    /** The editable settings draft owned by the page. Bound field-by-field. */
    draft: BridgeConfig;
    /** Persist the current draft. Resolves on success, throws on failure. */
    onSave: () => Promise<void>;
    onReset: () => void;
  } = $props();

  let open = $state(false);
  let saving = $state(false);

  async function save() {
    saving = true;
    try {
      await onSave();
      open = false;
    } finally {
      saving = false;
    }
  }
</script>

<section class="card collapsible">
  <button class="collapse-header" onclick={() => (open = !open)}>
    <h2>Settings</h2>
    <span class="chev" class:open aria-hidden="true">▾</span>
  </button>

  {#if open}
    <div class="collapse-body">
      <div class="settings">
        <label>
          <span>Mode</span>
          <select bind:value={draft.mode}>
            <option value="direct">Direct (OSC UDP)</option>
            <option value="satellite">Companion Satellite (TCP)</option>
          </select>
        </label>

        <label>
          <span>OSC feedback port</span>
          <input type="number" min="1" max="65535" bind:value={draft.feedback_port} />
        </label>

        <label>
          <span>HTTP config port</span>
          <input type="number" min="1" max="65535" bind:value={draft.config_port} />
        </label>

        <label>
          <span>Log level</span>
          <select bind:value={draft.log_level}>
            <option value="DEBUG">DEBUG</option>
            <option value="INFO">INFO</option>
            <option value="WARNING">WARNING</option>
            <option value="ERROR">ERROR</option>
          </select>
        </label>

        <label>
          <span>Satellite host</span>
          <input
            type="text"
            placeholder="companion host (satellite mode only)"
            bind:value={draft.satellite.host}
          />
        </label>

        <label>
          <span>Satellite port</span>
          <input type="number" min="1" max="65535" bind:value={draft.satellite.port} />
        </label>

        <div class="settings-actions">
          <button class="btn-primary" onclick={save} disabled={saving}>
            {saving ? 'Saving…' : 'Save settings'}
          </button>
          <button class="btn-ghost" onclick={onReset}>Reset</button>
        </div>
        <p class="muted small">
          Note: changing ports requires restarting the bridge app to take effect
          for the OSC listener and HTTP API.
        </p>
      </div>
    </div>
  {/if}
</section>
