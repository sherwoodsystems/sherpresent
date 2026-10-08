import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  type AppConfig,
  type LiveStatus,
  type AdapterType,
  type AdapterConfig,
  type NotesCache,
  type ConnectionStatus,
  type OscConfig,
  type DiscoveryConfig,
  type WebServerConfig,
  type DiscoveredPeer,
  type LatencyEvent,
  type CaptionsConfig,
  type CaptionSegment,
  type CaptionStatus,
  type OutputsStatus,
  type AudioDevice,
  defaultConfig
} from '$lib/types';

/** How many caption lines the in-app monitor keeps. */
const CAPTION_HISTORY = 40;

const isMac = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac');

class AppStore {
  config = $state<AppConfig>(defaultConfig);
  liveStatus = $state<LiveStatus | null>(null);
  discoveryRunning = $state(false);
  configLoaded = $state(false);
  connectionStatus = $state<ConnectionStatus>('Disconnected');
  webServerRunning = $state(false);
  webServerUrl = $state('');
  notesCache = $state<NotesCache>({});
  notesScanProgress = $state<{ current: number; total: number; status: string } | null>(null);
  latencyEvents = $state<LatencyEvent[]>([]);

  // Captions
  captionsRunning = $state(false);
  captionsUrl = $state('');
  captionStatus = $state<CaptionStatus | null>(null);
  /** Native video outputs (Syphon captions and notes); pushed as receivers come and go */
  outputsStatus = $state<OutputsStatus | null>(null);
  /** Newest last. The trailing entry may be an interim line still being spoken. */
  captionSegments = $state<CaptionSegment[]>([]);
  audioDevices = $state<AudioDevice[]>([]);

  private saveTimeout: ReturnType<typeof setTimeout> | null = null;
  private unlistenStatus: UnlistenFn | null = null;
  private unlistenCanvaLog: UnlistenFn | null = null;
  private unlistenNotes: UnlistenFn | null = null;
  private unlistenScanProgress: UnlistenFn | null = null;
  private unlistenLatency: UnlistenFn | null = null;
  private unlistenCaptionSegment: UnlistenFn | null = null;
  private unlistenCaptionStatus: UnlistenFn | null = null;
  private unlistenOutputsStatus: UnlistenFn | null = null;

  /**
   * PowerPoint for Mac can't jump to a slide during a show from AppleScript
   * (see docs/powerpoint-mac-goto-slide.md), so go-to is hidden there.
   */
  get supportsGoto() {
    return !(isMac && this.config.adapter === 'powerpoint');
  }

  async init() {
    try {
      const savedConfig = await invoke<AppConfig>('get_config');
      this.config = savedConfig;
      this.configLoaded = true;

      // The backend polls the saved presentation from startup; catch up on
      // what it already knows. Changes then arrive as events.
      this.liveStatus = await invoke<LiveStatus>('get_status');
      this.notesCache = await invoke<NotesCache>('get_all_notes');

      // Check discovery state on init
      if (savedConfig.discovery.enabled) {
        await this.startDiscovery();
      }

      // Check web server state on init
      await this.refreshWebServerStatus();
    } catch (e) {
      console.error('Failed to load config:', e);
      this.configLoaded = true;
    }

    // Status from the backend StateManager: polls, plus the optimistic
    // update of every slide command, whichever surface sent it.
    this.unlistenStatus = await listen<LiveStatus>('presentation-status', (event) => {
      this.liveStatus = event.payload;
    });

    // Notes cache updates (bulk fetch when a show starts, then per slide)
    this.unlistenNotes = await listen<NotesCache>('notes-cache-updated', (event) => {
      this.notesCache = event.payload;
    });

    // Listen for notes scan progress
    this.unlistenScanProgress = await listen<{ current: number; total: number; status: string }>(
      'notes-scan-progress',
      (event) => {
        this.notesScanProgress = event.payload;
        if (event.payload.status === 'complete' || event.payload.status === 'cancelled') {
          setTimeout(() => {
            this.notesScanProgress = null;
          }, 2000);
        }
      }
    );

    // Listen for Canva webview logs forwarded from Rust
    this.unlistenCanvaLog = await listen<{ category: string; message: string }>(
      'canva-webview-log',
      (event) => {
        const { category, message } = event.payload;
        console.log(`[CANVA:${category}]`, message);
      }
    );

    // Listen for latency events
    this.unlistenLatency = await listen<LatencyEvent>('latency-event', (event) => {
      this.latencyEvents = [event.payload, ...this.latencyEvents].slice(0, 50);
    });

    // Listen for caption lines. Interim segments reuse the id of the line they
    // replace, so match on id rather than appending blindly.
    this.unlistenCaptionSegment = await listen<CaptionSegment>('caption-segment', (event) => {
      const seg = event.payload;
      const existing = this.captionSegments;
      const last = existing[existing.length - 1];

      if (last && last.id === seg.id) {
        this.captionSegments = [...existing.slice(0, -1), seg];
      } else {
        this.captionSegments = [...existing, seg].slice(-CAPTION_HISTORY);
      }
    });

    this.unlistenCaptionStatus = await listen<CaptionStatus>('caption-status', (event) => {
      this.captionStatus = event.payload;
      const state = event.payload.state;
      this.captionsRunning =
        state === 'running' || state === 'starting' || state === 'reconnecting';
    });

    this.unlistenOutputsStatus = await listen<OutputsStatus>(
      'outputs-status',
      (event) => (this.outputsStatus = event.payload)
    );

    await this.refreshCaptionStatus();

    // Load any existing latency events
    try {
      const existing = await invoke<LatencyEvent[]>('get_latency_events');
      if (existing.length > 0) {
        this.latencyEvents = existing.reverse();
      }
    } catch (e) {
      console.error('Failed to load latency events:', e);
    }
  }

  destroy() {
    this.unlistenStatus?.();
    this.unlistenCanvaLog?.();
    this.unlistenNotes?.();
    this.unlistenScanProgress?.();
    this.unlistenLatency?.();
    this.unlistenCaptionSegment?.();
    this.unlistenCaptionStatus?.();
    this.unlistenOutputsStatus?.();
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
    }
  }

  private scheduleConfigSave() {
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
    }
    this.saveTimeout = setTimeout(async () => {
      try {
        await invoke('save_config', { config: this.config });
        console.log('Config saved');
      } catch (e) {
        console.error('Failed to save config:', e);
      }
    }, 500);
  }

  /**
   * Saving the config re-targets the backend, which clears the notes and
   * publishes the new presentation's status. Save now rather than after the
   * debounce so the switch is immediate.
   */
  async updateAdapter(adapter: AdapterType) {
    this.config = { ...this.config, adapter, presentationName: '' };
    this.liveStatus = null;
    await this.flushConfigSave().catch((e) => console.error('Failed to save config:', e));
  }

  async selectPresentation(name: string) {
    this.config = { ...this.config, presentationName: name };
    await this.flushConfigSave().catch((e) => console.error('Failed to save config:', e));
  }

  updateOscConfig(oscConfig: OscConfig) {
    this.config = { ...this.config, osc: oscConfig };
    this.scheduleConfigSave();
  }

  async updateDiscoveryConfig(discoveryConfig: DiscoveryConfig) {
    const wasEnabled = this.config.discovery.enabled;
    this.config = { ...this.config, discovery: discoveryConfig };
    this.scheduleConfigSave();

    if (discoveryConfig.enabled && !wasEnabled) {
      await this.startDiscovery();
    } else if (!discoveryConfig.enabled && wasEnabled) {
      await this.stopDiscovery();
    }
  }

  async startDiscovery() {
    if (!this.config.discovery.enabled) return;

    try {
      await invoke('start_discovery', {
        instanceId: this.config.discovery.instanceId,
        displayName: this.config.discovery.displayName,
        oscPort: this.config.osc.receivePort,
        networkInterface: this.config.discovery.networkInterface
      });
      this.discoveryRunning = true;
    } catch (e) {
      console.error('Failed to start discovery:', e);
    }
  }

  async stopDiscovery() {
    try {
      await invoke('stop_discovery');
      this.discoveryRunning = false;
    } catch (e) {
      console.error('Failed to stop discovery:', e);
    }
  }

  async getDiscoveredPeers(): Promise<DiscoveredPeer[]> {
    try {
      return await invoke<DiscoveredPeer[]>('get_discovered_peers');
    } catch (e) {
      console.error('Failed to get discovered peers:', e);
      return [];
    }
  }

  async setInstanceName(name: string | null): Promise<void> {
    await invoke('set_instance_name', { name });
  }

  /** Teleprompter-page the notes views (stage page and Syphon notes). */
  async pageNotes() {
    try {
      await invoke('page_notes');
    } catch (e) {
      console.error('Failed to page notes:', e);
    }
  }

  // Navigation goes through the backend StateManager, which publishes the
  // new position (optimistically, then confirmed) as `presentation-status`.

  async nextSlide() {
    await invoke('next_slide').catch((e) => console.error('Failed to go to next slide:', e));
  }

  async prevSlide() {
    await invoke('prev_slide').catch((e) => console.error('Failed to go to previous slide:', e));
  }

  async gotoSlide(slide: number) {
    await invoke('goto_slide', { slide }).catch((e) => console.error('Failed to go to slide:', e));
  }

  async fetchAllNotes() {
    if (!this.config.presentationName) return;
    try {
      this.notesCache = await invoke<NotesCache>('fetch_all_notes');
    } catch (e) {
      console.error('Failed to fetch all notes:', e);
    }
  }

  updateAdapterConfig(adapterConfig: AdapterConfig) {
    this.config = { ...this.config, adapterConfig };
    this.scheduleConfigSave();
  }

  async startNotesScan() {
    if (!this.config.presentationName) return;
    try {
      await invoke('start_notes_scan');
    } catch (e) {
      console.error('Failed to start notes scan:', e);
    }
  }

  async stopNotesScan() {
    try {
      await invoke('stop_notes_scan');
    } catch (e) {
      console.error('Failed to stop notes scan:', e);
    }
  }

  async clearLatencyEvents() {
    try {
      await invoke('clear_latency_events');
      this.latencyEvents = [];
    } catch (e) {
      console.error('Failed to clear latency events:', e);
    }
  }

  async openCanvaRemote(url: string) {
    try {
      await invoke('open_canva_remote', { url });
      this.connectionStatus = 'Connected';
    } catch (e) {
      console.error('Failed to open Canva remote:', e);
      this.connectionStatus = { Error: String(e) };
    }
  }

  async closeCanvaRemote() {
    try {
      await invoke('close_canva_remote');
      this.connectionStatus = 'Disconnected';
    } catch (e) {
      console.error('Failed to close Canva remote:', e);
    }
  }

  async refreshConnectionStatus() {
    if (this.config.adapter === 'canva') {
      try {
        this.connectionStatus = await invoke<ConnectionStatus>('get_canva_connection_status');
      } catch {
        this.connectionStatus = 'Disconnected';
      }
    }
  }

  updateWebServerConfig(webServer: WebServerConfig) {
    this.config = { ...this.config, webServer };
    this.scheduleConfigSave();
  }

  async startWebServer() {
    try {
      const url = await invoke<string>('start_web_server');
      this.webServerRunning = true;
      this.webServerUrl = url;
    } catch (e) {
      console.error('Failed to start web server:', e);
    }
  }

  async refreshWebServerStatus() {
    try {
      this.webServerRunning = await invoke<boolean>('is_web_server_running');
      if (this.webServerRunning) {
        this.webServerUrl = await invoke<string>('get_web_server_url');
      }
    } catch {
      this.webServerRunning = false;
    }
  }

  // ===========================================================================
  // CAPTIONS
  // ===========================================================================

  updateCaptionsConfig(captions: CaptionsConfig) {
    this.config = { ...this.config, captions };
    this.scheduleConfigSave();
  }

  async loadAudioDevices() {
    try {
      this.audioDevices = await invoke<AudioDevice[]>('list_audio_input_devices');
    } catch (e) {
      console.error('Failed to list audio input devices:', e);
      this.audioDevices = [];
    }
  }

  /**
   * Start captions. Config is read from disk by the backend, so flush any
   * pending debounced save first or a key typed seconds ago would be missed.
   */
  async startCaptions(): Promise<string | null> {
    try {
      // `enabled` records the operator's intent so captions come back after a
      // restart, the same way polling and discovery do.
      this.config = { ...this.config, captions: { ...this.config.captions, enabled: true } };
      await this.flushConfigSave();
      await invoke('start_captions');
      this.captionSegments = [];
      this.captionsRunning = true;
      await this.refreshCaptionsUrl();
      return null;
    } catch (e) {
      console.error('Failed to start captions:', e);
      this.captionsRunning = false;
      // Don't let a failed start leave the app auto-starting into the same
      // failure on every launch.
      this.config = { ...this.config, captions: { ...this.config.captions, enabled: false } };
      this.scheduleConfigSave();
      return String(e);
    }
  }

  async stopCaptions() {
    try {
      await invoke('stop_captions');
    } catch (e) {
      console.error('Failed to stop captions:', e);
    }
    this.captionsRunning = false;
    this.config = { ...this.config, captions: { ...this.config.captions, enabled: false } };
    this.scheduleConfigSave();
  }

  async refreshCaptionStatus() {
    try {
      this.captionsRunning = await invoke<boolean>('is_captions_running');
      this.captionStatus = await invoke<CaptionStatus>('get_caption_status');
      this.outputsStatus = await invoke<OutputsStatus>('get_outputs_status');
      await this.refreshCaptionsUrl();
    } catch (e) {
      console.error('Failed to read caption status:', e);
    }
  }

  async refreshCaptionsUrl() {
    try {
      this.captionsUrl = await invoke<string>('get_captions_url');
    } catch {
      this.captionsUrl = '';
    }
  }

  /** Write config now instead of waiting out the debounce. */
  private async flushConfigSave() {
    if (this.saveTimeout) {
      clearTimeout(this.saveTimeout);
      this.saveTimeout = null;
    }
    await invoke('save_config', { config: this.config });
  }
}

export const appStore = new AppStore();
