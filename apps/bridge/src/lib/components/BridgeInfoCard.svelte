<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { BridgeInfo } from '$lib/types';

  let {
    info,
    onRename,
    onCopy,
  }: {
    info: BridgeInfo;
    /** Persist a new bridge name. Resolves on success, throws on failure. */
    onRename: (name: string) => Promise<void>;
    onCopy: (text: string) => void;
  } = $props();

  let editing = $state(false);
  let draft = $state('');
  let saving = $state(false);

  function startEdit() {
    draft = info.bridge_name;
    editing = true;
  }

  function cancelEdit() {
    draft = info.bridge_name;
    editing = false;
  }

  async function save() {
    saving = true;
    try {
      await onRename(draft);
      editing = false;
    } finally {
      saving = false;
    }
  }
</script>

<section class="card">
  <h2>This Bridge</h2>
  <dl class="info-grid">
    <dt>Name</dt>
    <dd class="name-row">
      {#if editing}
        <input
          type="text"
          bind:value={draft}
          onkeydown={(e) => e.key === 'Enter' && save()}
          disabled={saving}
          placeholder="Bridge name"
        />
        <button class="btn-primary" onclick={save} disabled={saving}>
          {saving ? 'Saving…' : 'Save'}
        </button>
        <button class="btn-ghost" onclick={cancelEdit} disabled={saving}>Cancel</button>
      {:else}
        <span class="name-value">{info.bridge_name}</span>
        <button class="btn-ghost" onclick={startEdit}>Rename</button>
      {/if}
    </dd>

    <dt>Bridge ID</dt>
    <dd class="mono"><code>{info.bridge_id}</code></dd>

    <dt>Mode</dt>
    <dd><span class="chip">{info.mode}</span> <span class="chip">{info.log_level}</span></dd>

    <dt>OSC feedback</dt>
    <dd>UDP <code>{info.feedback_port}</code></dd>

    <dt>Config API</dt>
    <dd>
      TCP <code>{info.config_port}</code>
      {#if info.lan_ip}
        <span class="muted">— accessible at <a
          href={info.config_url}
          onclick={(e) => { e.preventDefault(); openUrl(info.config_url); }}
        >{info.config_url}</a>
          <button class="btn-tiny" onclick={() => onCopy(info.config_url)}>Copy</button>
        </span>
      {/if}
    </dd>

    <dt>LAN IP</dt>
    <dd class="mono">{info.lan_ip ?? 'unknown'}</dd>
  </dl>
</section>
