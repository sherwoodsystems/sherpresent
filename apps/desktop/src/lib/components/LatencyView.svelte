<script lang="ts">
  import type { LatencyEvent } from '../types';

  interface Props {
    events: LatencyEvent[];
    onclear: () => void;
  }

  let { events, onclear }: Props = $props();

  const stats = $derived.by(() => {
    if (events.length === 0) return null;
    const latencies = events.map(e => e.latency_ms);
    return {
      count: latencies.length,
      min: Math.min(...latencies),
      max: Math.max(...latencies),
      avg: Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length),
    };
  });

  function formatTime(wallClockMs: number): string {
    return new Date(wallClockMs).toLocaleTimeString();
  }

  function latencyClass(ms: number): string {
    if (ms < 200) return 'latency-good';
    if (ms < 500) return 'latency-warn';
    return 'latency-bad';
  }

  function sourceLabel(source: string): string {
    switch (source) {
      case 'osc': return 'OSC';
      case 'osc_broadcast': return 'Broadcast';
      case 'ui': return 'UI';
      default: return source;
    }
  }
</script>

<div class="latency-view">
  <div class="header">
    <h3 class="section-title">Command Latency</h3>
    {#if events.length > 0}
      <button class="clear-btn" onclick={onclear}>Clear</button>
    {/if}
  </div>

  {#if stats}
    <div class="stats-bar">
      <span class="stat"><strong>{stats.count}</strong> events</span>
      <span class="stat">Min: <strong>{stats.min}ms</strong></span>
      <span class="stat">Avg: <strong>{stats.avg}ms</strong></span>
      <span class="stat">Max: <strong>{stats.max}ms</strong></span>
    </div>
  {/if}

  {#if events.length === 0}
    <p class="placeholder">No latency events yet. Send a slide command to see measurements.</p>
  {:else}
    <div class="table-wrapper">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Command</th>
            <th>Source</th>
            <th>Adapter</th>
            <th>Latency</th>
          </tr>
        </thead>
        <tbody>
          {#each events as event (event.wall_clock_ms)}
            <tr>
              <td class="mono">{formatTime(event.wall_clock_ms)}</td>
              <td class="mono">{event.command}</td>
              <td><span class="badge badge-{event.source}">{sourceLabel(event.source)}</span></td>
              <td>{event.adapter}</td>
              <td class="mono {latencyClass(event.latency_ms)}">{event.latency_ms}ms</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .latency-view {
    padding: 1rem;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }

  .section-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }

  .clear-btn {
    padding: 0.25rem 0.75rem;
    font-size: 0.75rem;
    border: 1px solid #ccc;
    border-radius: 4px;
    background: #fff;
    cursor: pointer;
  }

  .clear-btn:hover {
    background: #f0f0f0;
  }

  .stats-bar {
    display: flex;
    gap: 1rem;
    padding: 0.5rem 0.75rem;
    background: #f5f5f5;
    border-radius: 6px;
    margin-bottom: 0.75rem;
    font-size: 0.8rem;
  }

  .stat strong {
    font-weight: 600;
  }

  .placeholder {
    color: #999;
    font-size: 0.875rem;
    text-align: center;
    padding: 2rem 0;
  }

  .table-wrapper {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-bottom: 2px solid #e0e0e0;
    font-weight: 600;
    font-size: 0.75rem;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  td {
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid #f0f0f0;
  }

  .mono {
    font-family: 'SF Mono', 'Menlo', 'Consolas', monospace;
    font-size: 0.75rem;
  }

  .badge {
    display: inline-block;
    padding: 0.1rem 0.4rem;
    border-radius: 3px;
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
  }

  .badge-ui {
    background: #e3f2fd;
    color: #1565c0;
  }

  .badge-osc {
    background: #e8f5e9;
    color: #2e7d32;
  }

  .badge-osc_broadcast {
    background: #fff3e0;
    color: #e65100;
  }

  .latency-good {
    color: #2e7d32;
  }

  .latency-warn {
    color: #f57f17;
  }

  .latency-bad {
    color: #c62828;
    font-weight: 600;
  }

  @media (prefers-color-scheme: dark) {
    .clear-btn {
      background: #333;
      border-color: #555;
      color: #eee;
    }

    .clear-btn:hover {
      background: #444;
    }

    .stats-bar {
      background: #2a2a2a;
    }

    .placeholder {
      color: #666;
    }

    th {
      border-bottom-color: #444;
      color: #999;
    }

    td {
      border-bottom-color: #333;
    }

    .badge-ui {
      background: #1a3a5c;
      color: #90caf9;
    }

    .badge-osc {
      background: #1b3b1f;
      color: #a5d6a7;
    }

    .badge-osc_broadcast {
      background: #3e2a1a;
      color: #ffcc80;
    }

    .latency-good {
      color: #66bb6a;
    }

    .latency-warn {
      color: #fdd835;
    }

    .latency-bad {
      color: #ef5350;
    }
  }
</style>
