<script lang="ts">
  import Card from '$lib/components/ui/Card.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import Notice from '$lib/components/ui/Notice.svelte';
  import PageHeader from '$lib/components/ui/PageHeader.svelte';
  import StatusLine from '$lib/components/ui/StatusLine.svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from '$lib/components/ConnectionInfo.svelte';
  import SyphonOutputField from '$lib/components/SyphonOutputField.svelte';
  import { formatCueList, formatTime, listFound, parseCueList, remainingPercent } from '$lib/qlab';
  import type { FoundQLabWorkspace, QLabConfig } from '$lib/types';

  const config = $derived(appStore.config.qlab);
  const live = $derived(appStore.qlabState);
  const syphonStatus = $derived(appStore.outputsStatus?.qlab);
  const pageUrl = $derived(appStore.webServerUrl ? `${appStore.webServerUrl}/qlab` : '');

  const SCAN_EVERY_MS = 2500;
  let lastScan = $state<FoundQLabWorkspace[]>([]);
  let found = $state<FoundQLabWorkspace[]>([]);
  let scanError = $state<string | null>(null);

  // Scans only while this tab is open.
  $effect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    async function scan() {
      try {
        const latest = await invoke<FoundQLabWorkspace[]>('find_qlab_workspaces');
        if (stopped) return;
        found = listFound(lastScan, latest);
        lastScan = latest;
        scanError = null;
      } catch (e) {
        if (stopped) return;
        scanError = String(e);
      }
      timer = setTimeout(scan, SCAN_EVERY_MS);
    }
    scan();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  });

  function isSelected(w: FoundQLabWorkspace) {
    return config.host === w.host && config.workspace === w.name;
  }

  const connectionTone = $derived.by(() => {
    if (!config.host.trim()) return 'idle';
    return live?.connected ? 'ok' : 'bad';
  });

  const statusText = $derived.by(() => {
    if (!config.host.trim()) return 'Not configured — enter the QLab machine’s IP';
    if (!live) return 'Connecting…';
    if (live.connected) return `Connected to “${live.workspace ?? 'QLab'}”`;
    return live.message ?? 'Connecting…';
  });

  // Picked workspace that QLab no longer lists still shows, so the select
  // never silently changes what's saved.
  const workspaceOptions = $derived.by(() => {
    const names = [...(live?.workspaces ?? [])];
    if (config.workspace && !names.includes(config.workspace)) names.unshift(config.workspace);
    return names;
  });

  // For the prefix hint: what slide 3 would fire.
  const exampleSlide = 3;
  const cuePrefix = $derived(config.slideCuePrefix.replace(/[\s/]/g, ''));

  function update(patch: Partial<QLabConfig>) {
    appStore.updateQLabConfig({ ...config, ...patch });
  }

  const counts = $derived(
    [
      live?.video ? { kind: 'video' as const, cue: live.video } : null,
      live?.audio ? { kind: 'audio' as const, cue: live.audio } : null
    ].filter((c) => c !== null)
  );
</script>

<main class="page">
  <PageHeader
    title="QLab Count-out"
    subtitle="Time left on running video and audio cues. Looping cues are ignored, so backgrounds don't hide the real count."
  />

  <Card title="Connection">
    <Field label="Open on this network">
      <div class="found">
        {#each found as w (`${w.host} ${w.id}`)}
          <button
            type="button"
            class="found-row"
            class:selected={isSelected(w)}
            aria-pressed={isSelected(w)}
            onclick={() => update({ host: w.host, workspace: w.name })}
          >
            <span class="found-name">{isSelected(w) ? '✓ ' : ''}{w.name}</span>
            <span class="found-where">
              {w.host === '127.0.0.1' ? 'This computer' : w.host} · QLab {w.version}
            </span>
          </button>
        {:else}
          <span class="hint">
            {scanError ?? 'Looking for QLab… On another VLAN or subnet, enter its IP below.'}
          </span>
        {/each}
      </div>
    </Field>
    <div class="config-grid">
      <Field
        label="QLab Host"
        for="qlab-host"
        hint="IP of the Mac running QLab; leave empty to turn off"
      >
        <input
          id="qlab-host"
          class="input mono"
          value={config.host}
          placeholder="e.g. 192.168.1.20"
          onchange={(e) => update({ host: e.currentTarget.value.trim() })}
        />
      </Field>
      <Field label="OSC Port" for="qlab-port" hint="TCP; QLab's default is 53000">
        <input
          id="qlab-port"
          type="number"
          class="input mono"
          value={config.port}
          min="1"
          max="65535"
          onchange={(e) => update({ port: parseInt(e.currentTarget.value) || 53000 })}
        />
      </Field>
      <Field
        label="OSC Passcode"
        for="qlab-passcode"
        hint="Workspace Settings → Network → passcode, if set"
      >
        <input
          id="qlab-passcode"
          type="password"
          class="input mono"
          value={config.passcode}
          placeholder="None"
          autocomplete="off"
          onchange={(e) => update({ passcode: e.currentTarget.value })}
        />
      </Field>
      <Field label="Workspace" for="qlab-workspace">
        <select
          id="qlab-workspace"
          class="input"
          value={config.workspace}
          onchange={(e) => update({ workspace: e.currentTarget.value })}
        >
          <option value="">First open workspace</option>
          {#each workspaceOptions as name (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
      </Field>
      <Field
        label="Hide Cues"
        for="qlab-hidden"
        hint="Cue numbers never counted out, separated by commas or spaces"
        span
      >
        <input
          id="qlab-hidden"
          class="input mono"
          value={formatCueList(config.hiddenCues)}
          placeholder="e.g. 12, 14, 3.5"
          onchange={(e) => update({ hiddenCues: parseCueList(e.currentTarget.value) })}
        />
      </Field>
    </div>

    <StatusLine tone={connectionTone}>{statusText}</StatusLine>
  </Card>

  <Card title="On Slide Change">
    <Checkbox
      label="Start the matching cue in QLab"
      checked={config.slideGo}
      onchange={(slideGo) => update({ slideGo })}
    />
    <Field label="Cue Number Prefix" for="qlab-cue-prefix">
      <input
        id="qlab-cue-prefix"
        class="input mono prefix"
        value={config.slideCuePrefix}
        placeholder="None"
        spellcheck="false"
        disabled={!config.slideGo}
        onchange={(e) => update({ slideCuePrefix: e.currentTarget.value.replace(/[\s/]/g, '') })}
      />
      <span class="hint">
        Slide {exampleSlide} fires <code>/cue/{cuePrefix}{exampleSlide}/start</code>: QLab starts
        cue
        <strong>{cuePrefix}{exampleSlide}</strong> and leaves the playhead where it is. Each build
        on the slide starts its own cue: <strong>{cuePrefix}{exampleSlide}.1</strong>,
        <strong>{cuePrefix}{exampleSlide}.2</strong>… (PowerPoint for Windows). Going back starts
        the cue for wherever it lands. Fired the moment the slide changes during a show, not when it
        starts or ends. A step with no matching cue does nothing.
      </span>
    </Field>
    <Field label="Also Send (optional)" for="qlab-slide-command">
      <textarea
        id="qlab-slide-command"
        class="input mono"
        rows="2"
        value={config.slideCommand}
        placeholder={'e.g. /cue/FX{slide}/start'}
        spellcheck="false"
        onchange={(e) => update({ slideCommand: e.currentTarget.value })}></textarea>
      <span class="hint">
        One OSC message per line, arguments after a space (quote one to send it as text);
        <code>{'{slide}'}</code> becomes the slide number. Sent on slide changes, not builds.
      </span>
    </Field>
    <span class="hint">
      Sent to the workspace above over its connection, so its passcode needs control access.
    </span>
    {#if live?.slideTrigger}
      {@const t = live.slideTrigger}
      <StatusLine tone={t.error ? 'bad' : 'ok'} mono>
        Slide {t.step}: {t.error ?? t.sent.join(', ')}
      </StatusLine>
    {/if}
  </Card>

  {#if live?.connected}
    <Card title="Live">
      {#if counts.length === 0}
        <p class="empty">Nothing playing</p>
        <div class="next">
          <span class="next-tag">NEXT</span>
          {#if live.next}
            <strong>{live.next.number}</strong>
            <span>{live.next.name}</span>
          {:else}
            <span class="faint">No cue at the playhead</span>
          {/if}
        </div>
      {:else}
        {#each counts as { kind, cue } (kind)}
          <div class="count {kind}" class:paused={cue.paused}>
            <div class="count-head">
              <span class="kind-tag">{kind.toUpperCase()}</span>
              <span class="cue-name">{[cue.number, cue.name].filter(Boolean).join(' · ')}</span>
              {#if cue.paused}<span class="paused-tag">PAUSED</span>{/if}
              {#if cue.others > 0}<span class="faint">+{cue.others} more</span>{/if}
            </div>
            <div class="count-time">{formatTime(cue.remaining)}</div>
            <div class="track">
              <div class="fill" style="width: {remainingPercent(cue)}%"></div>
            </div>
            <div class="count-total">of {formatTime(cue.duration)}</div>
          </div>
        {/each}
      {/if}
    </Card>
  {/if}

  <Card title="Outputs">
    {#if pageUrl}
      <ConnectionInfo
        label="Count-out page available at:"
        value={pageUrl}
        onopen={() => openUrl(pageUrl)}
      />
    {:else}
      <Notice>
        The web server isn't running, so the count-out page isn't available. Port
        {appStore.config.webServer.port} may be in use by another app — change it under Settings → Stage
        View Server and restart SherPresent.
      </Notice>
    {/if}
    {#if syphonStatus?.supported && appStore.configLoaded}
      <SyphonOutputField
        id="qlab-syphon"
        label="Syphon count-out output"
        config={config.syphon}
        status={syphonStatus}
        defaultName="SherPresent QLab"
        onchange={(syphon) => update({ syphon })}
      />
      <span class="hint">The same picture as the page, as a 1920×1080 source</span>
    {/if}
  </Card>
</main>

<style>
  .found {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .found-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.25rem 0.75rem;
    padding: 0.5rem 0.75rem;
    font: inherit;
    font-size: 0.875rem;
    text-align: left;
    color: var(--info-text);
    background: var(--info-bg);
    border: 2px solid var(--info-border);
    border-radius: 6px;
    cursor: pointer;
    transition: border-color 0.15s ease;
  }

  .found-row:hover,
  .found-row:focus-visible,
  .found-row.selected {
    border-color: var(--accent);
    outline: none;
  }

  .found-name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .found-where {
    font-size: 0.75rem;
    opacity: 0.8;
  }

  .prefix {
    max-width: 8rem;
  }

  .next {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0.75rem;
    border-radius: 8px;
    background: var(--bg);
    font-size: 1rem;
  }

  .next-tag,
  .kind-tag,
  .paused-tag {
    font-size: 0.65rem;
    font-weight: 700;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    background: var(--border-subtle);
    color: var(--text-secondary);
  }

  .faint {
    color: var(--text-muted);
    font-size: 0.85rem;
  }

  /* Same colours as the page and the Syphon output. */
  .count {
    --kind: #38bdf8;
    background: #0b0b0c;
    color: #ddd;
    border-radius: 10px;
    padding: 0.75rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .count.audio {
    --kind: #fbbf24;
  }

  .count.paused {
    --kind: #777;
  }

  .count-head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
    min-width: 0;
  }

  .count .kind-tag {
    background: var(--kind);
    color: #000;
  }

  .cue-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count-time {
    font-size: 3.5rem;
    font-weight: 700;
    line-height: 1;
    text-align: center;
    color: var(--kind);
    font-variant-numeric: tabular-nums;
  }

  .track {
    height: 8px;
    border-radius: 4px;
    background: #262626;
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--kind);
  }

  .count-total {
    text-align: right;
    font-size: 0.75rem;
    color: #777;
    font-variant-numeric: tabular-nums;
  }
</style>
