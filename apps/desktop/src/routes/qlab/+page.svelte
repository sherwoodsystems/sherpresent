<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { appStore } from '$lib/state.svelte';
  import ConnectionInfo from '$lib/components/ConnectionInfo.svelte';
  import SyphonOutputField from '$lib/components/SyphonOutputField.svelte';
  import { formatCueList, formatTime, parseCueList, remainingPercent } from '$lib/qlab';
  import type { QLabConfig } from '$lib/types';

  const config = $derived(appStore.config.qlab);
  const live = $derived(appStore.qlabState);
  const syphonStatus = $derived(appStore.outputsStatus?.qlab);
  const pageUrl = $derived(appStore.webServerUrl ? `${appStore.webServerUrl}/qlab` : '');

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

<main class="container">
  <header class="header">
    <h1>QLab Count-out</h1>
    <p class="subtitle">
      Time left on running video and audio cues. Looping cues are ignored, so backgrounds don't hide
      the real count.
    </p>
  </header>

  <section class="section">
    <h3 class="section-title">Connection</h3>
    <div class="config-grid">
      <div class="field">
        <label class="label" for="qlab-host">QLab Host</label>
        <input
          id="qlab-host"
          class="input mono"
          value={config.host}
          placeholder="e.g. 192.168.1.20"
          onchange={(e) => update({ host: e.currentTarget.value.trim() })}
        />
        <span class="hint">IP of the Mac running QLab; leave empty to turn off</span>
      </div>
      <div class="field">
        <label class="label" for="qlab-port">OSC Port</label>
        <input
          id="qlab-port"
          type="number"
          class="input mono"
          value={config.port}
          min="1"
          max="65535"
          onchange={(e) => update({ port: parseInt(e.currentTarget.value) || 53000 })}
        />
        <span class="hint">TCP; QLab's default is 53000</span>
      </div>
      <div class="field">
        <label class="label" for="qlab-passcode">OSC Passcode</label>
        <input
          id="qlab-passcode"
          type="password"
          class="input mono"
          value={config.passcode}
          placeholder="None"
          autocomplete="off"
          onchange={(e) => update({ passcode: e.currentTarget.value })}
        />
        <span class="hint">Workspace Settings → Network → passcode, if set</span>
      </div>
      <div class="field">
        <label class="label" for="qlab-workspace">Workspace</label>
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
      </div>
      <div class="field span">
        <label class="label" for="qlab-hidden">Hide Cues</label>
        <input
          id="qlab-hidden"
          class="input mono"
          value={formatCueList(config.hiddenCues)}
          placeholder="e.g. 12, 14, 3.5"
          onchange={(e) => update({ hiddenCues: parseCueList(e.currentTarget.value) })}
        />
        <span class="hint">Cue numbers never counted out, separated by commas or spaces</span>
      </div>
    </div>

    <div class="status-row">
      <span
        class="status-dot"
        class:ok={live?.connected}
        class:idle={!config.host.trim()}
        class:bad={!!config.host.trim() && !live?.connected}
      ></span>
      <span class="status-text">{statusText}</span>
    </div>
  </section>

  <section class="section">
    <h3 class="section-title">On Slide Change</h3>
    <label class="check-row">
      <input
        type="checkbox"
        checked={config.slideGo}
        onchange={(e) => update({ slideGo: e.currentTarget.checked })}
      />
      <span class="label">Start the matching cue in QLab</span>
    </label>
    <div class="field">
      <label class="label" for="qlab-cue-prefix">Cue Number Prefix</label>
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
    </div>
    <div class="field">
      <label class="label" for="qlab-slide-command">Also Send (optional)</label>
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
    </div>
    <span class="hint">
      Sent to the workspace above over its connection, so its passcode needs control access.
    </span>
    {#if live?.slideTrigger}
      {@const t = live.slideTrigger}
      <div class="status-row">
        <span class="status-dot" class:ok={!t.error} class:bad={!!t.error}></span>
        <span class="status-text mono">
          Slide {t.step}: {t.error ?? t.sent.join(', ')}
        </span>
      </div>
    {/if}
  </section>

  {#if live?.connected}
    <section class="section">
      <h3 class="section-title">Live</h3>
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
    </section>
  {/if}

  <section class="section">
    <h3 class="section-title">Outputs</h3>
    {#if pageUrl}
      <ConnectionInfo label="Count-out page available at:" value={pageUrl} />
      <div>
        <button class="secondary" onclick={() => openUrl(pageUrl)}>Open Count-out Page</button>
      </div>
    {:else}
      <p class="notice">
        The web server isn't running, so the count-out page isn't available. Port
        {appStore.config.webServer.port} may be in use by another app — change it under Settings → Stage
        View Server and restart SherPresent.
      </p>
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
  </section>
</main>

<style>
  .container {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .header {
    text-align: center;
  }

  .header h1 {
    margin: 0;
    font-size: 1.5rem;
    font-weight: 700;
  }

  .subtitle {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .section {
    background: var(--surface);
    border-radius: 12px;
    padding: 1rem;
    box-shadow: var(--shadow-card);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .input {
    width: 100%;
    box-sizing: border-box;
  }

  .mono {
    font-family: monospace;
  }

  textarea.input {
    resize: vertical;
  }

  .prefix {
    max-width: 8rem;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .span {
    grid-column: 1 / -1;
  }

  .status-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .status-dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warning);
  }

  .status-dot.ok {
    background: #34c759;
  }

  .status-dot.idle {
    background: #8e8e93;
  }

  .status-dot.bad {
    background: #ff3b30;
  }

  .status-text {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .secondary {
    padding: 0.5rem 0.85rem;
    border: 2px solid var(--border);
    border-radius: 6px;
    background: var(--input-bg);
    color: var(--text-secondary);
    font-size: 0.8rem;
    font-weight: 500;
    cursor: pointer;
  }

  .notice {
    margin: 0;
    font-size: 0.8rem;
    color: #8a6d00;
    background: #fff8e1;
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
  }

  .empty {
    margin: 0;
    text-align: center;
    font-style: italic;
    color: var(--text-muted);
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
