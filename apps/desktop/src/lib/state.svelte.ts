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
  type SlideInfo,
  type LatencyEvent,
  defaultConfig
} from '$lib/types';

class AppStore {
  config = $state<AppConfig>(defaultConfig);
  liveStatus = $state<LiveStatus | null>(null);
  pollingActive = $state(false);
  discoveryRunning = $state(false);
  configLoaded = $state(false);
  connectionStatus = $state<ConnectionStatus>('Disconnected');
  webServerRunning = $state(false);
  webServerUrl = $state('');
  notesCache = $state<NotesCache>({});
  notesScanProgress = $state<{ current: number; total: number; status: string } | null>(null);
  latencyEvents = $state<LatencyEvent[]>([]);

  private saveTimeout: ReturnType<typeof setTimeout> | null = null;
  private unlistenStatus: UnlistenFn | null = null;
  private unlistenCanvaLog: UnlistenFn | null = null;
  private unlistenNotes: UnlistenFn | null = null;
  private unlistenScanProgress: UnlistenFn | null = null;
  private unlistenLatency: UnlistenFn | null = null;
  private navigatingUntil = 0;

  async init() {
    try {
      const savedConfig = await invoke<AppConfig>('get_config');
      this.config = savedConfig;
      this.configLoaded = true;

      // Auto-start polling if presentation was already selected
      if (savedConfig.presentationName) {
        await this.startPolling();
      }
      
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

    // Listen for status updates
    this.unlistenStatus = await listen<LiveStatus>('presentation-status', (event) => {
      if (Date.now() < this.navigatingUntil) return; // Skip stale updates during transition
      this.liveStatus = event.payload;
    });

    // Listen for notes cache updates (progressive fill from polling)
    this.unlistenNotes = await listen<NotesCache>('notes-cache-updated', (event) => {
      console.log('[notes] polling cache update:', Object.keys(event.payload).length, 'entries, keys:', Object.keys(event.payload));
      this.notesCache = event.payload;
    });

    // Listen for notes scan progress
    this.unlistenScanProgress = await listen<{ current: number; total: number; status: string }>('notes-scan-progress', (event) => {
      this.notesScanProgress = event.payload;
      if (event.payload.status === 'complete' || event.payload.status === 'cancelled') {
        setTimeout(() => { this.notesScanProgress = null; }, 2000);
      }
    });

    // Listen for Canva webview logs forwarded from Rust
    this.unlistenCanvaLog = await listen<{ category: string; message: string }>('canva-webview-log', (event) => {
      const { category, message } = event.payload;
      console.log(`[CANVA:${category}]`, message);
    });

    // Listen for latency events
    this.unlistenLatency = await listen<LatencyEvent>('latency-event', (event) => {
      this.latencyEvents = [event.payload, ...this.latencyEvents].slice(0, 50);
    });

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

  async updateAdapter(adapter: AdapterType) {
    if (this.pollingActive) {
      await this.stopPolling();
    }
    this.config = { ...this.config, adapter, presentationName: '' };
    this.liveStatus = null;
    await this.clearNotesCache();
    this.scheduleConfigSave();
  }

  async selectPresentation(name: string) {
    console.log('[selectPresentation] called with:', name, '| pollingActive:', this.pollingActive);
    if (this.pollingActive) {
      console.log('[selectPresentation] stopping existing polling...');
      await this.stopPolling();
      console.log('[selectPresentation] polling stopped');
    }

    console.log('[selectPresentation] clearing notes cache...');
    await this.clearNotesCache();
    console.log('[selectPresentation] notes cache cleared');
    this.config = { ...this.config, presentationName: name };
    this.scheduleConfigSave();

    if (name) {
      console.log('[selectPresentation] starting polling for:', name);
      await this.startPolling();
      console.log('[selectPresentation] polling started');
      // Notes are fetched by the polling background thread when presenting is detected,
      // and delivered via 'notes-cache-updated' event. Calling fetchAllNotes() here would
      // race with the polling thread for the AppleScript EXECUTION_LOCK, blocking the
      // Tauri IPC thread and freezing the UI.
    }
    console.log('[selectPresentation] done');
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

  async startPolling() {
    if (!this.config.presentationName) {
      alert('Please select a presentation first');
      return;
    }
    try {
      await invoke('start_status_polling', {
        adapter: this.config.adapter,
        presentationName: this.config.presentationName
      });
      this.pollingActive = true;
    } catch (e) {
      console.error('Failed to start polling:', e);
    }
  }

  async stopPolling() {
    try {
      await invoke('stop_status_polling');
      this.pollingActive = false;
    } catch (e) {
      console.error('Failed to stop polling:', e);
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

  async nextSlide() {
    try {
      const oldSlide = this.liveStatus?.current_slide ?? 0;
      const info = await invoke<SlideInfo>('next_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
      if (this.liveStatus && info) {
        let displaySlide = info.current;
        // If the adapter returned the old slide number and a transition is in progress,
        // optimistically show the next slide number
        if (info.current === oldSlide && (info.transition_duration ?? 0) > 0) {
          displaySlide = oldSlide + 1;
        }
        this.navigatingUntil = Date.now() + ((info.transition_duration ?? 0) * 1000 || 1500);
        this.liveStatus = { ...this.liveStatus, current_slide: displaySlide, total_slides: info.total };
      }
    } catch (e) {
      console.error('Failed to go to next slide:', e);
    }
  }

  async prevSlide() {
    try {
      const oldSlide = this.liveStatus?.current_slide ?? 0;
      const info = await invoke<SlideInfo>('prev_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
      if (this.liveStatus && info) {
        let displaySlide = info.current;
        if (info.current === oldSlide && oldSlide > 1) {
          displaySlide = oldSlide - 1;
        }
        this.navigatingUntil = Date.now() + ((info.transition_duration ?? 0) * 1000 || 1500);
        this.liveStatus = { ...this.liveStatus, current_slide: displaySlide, total_slides: info.total };
      }
    } catch (e) {
      console.error('Failed to go to previous slide:', e);
    }
  }

  async gotoSlide(slide: number) {
    try {
      const oldSlide = this.liveStatus?.current_slide ?? 0;
      const info = await invoke<SlideInfo>('goto_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName,
        slide
      });
      if (this.liveStatus && info) {
        let displaySlide = info.current;
        // If the adapter returned the old slide number, optimistically use the target
        if (info.current === oldSlide && info.current !== slide) {
          displaySlide = slide;
        }
        this.navigatingUntil = Date.now() + ((info.transition_duration ?? 0) * 1000 || 1500);
        this.liveStatus = { ...this.liveStatus, current_slide: displaySlide, total_slides: info.total };
      }
    } catch (e) {
      console.error('Failed to go to slide:', e);
    }
  }

  async fetchAllNotes() {
    if (!this.config.presentationName) return;
    try {
      const result = await invoke<NotesCache>('fetch_all_notes', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
      console.log('[notes] fetchAllNotes result:', Object.keys(result).length, 'entries, keys:', Object.keys(result), 'values preview:', Object.fromEntries(Object.entries(result).map(([k, v]) => [k, v.substring(0, 50)])));
      this.notesCache = result;
    } catch (e) {
      console.error('Failed to fetch all notes:', e);
    }
  }

  async clearNotesCache() {
    try {
      await invoke('clear_notes_cache');
      this.notesCache = {};
    } catch (e) {
      console.error('Failed to clear notes cache:', e);
    }
  }

  updateAdapterConfig(adapterConfig: AdapterConfig) {
    this.config = { ...this.config, adapterConfig };
    this.scheduleConfigSave();
  }

  async startNotesScan() {
    if (!this.config.presentationName) return;
    try {
      await invoke('start_notes_scan', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
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
}

export const appStore = new AppStore();
