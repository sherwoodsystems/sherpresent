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
  type ChannelConfig,
  type DiscoveredPeer,
  defaultConfig
} from '$lib/types';

class AppStore {
  config = $state<AppConfig>(defaultConfig);
  liveStatus = $state<LiveStatus | null>(null);
  pollingActive = $state(false);
  discoveryRunning = $state(false);
  configLoaded = $state(false);
  connectionStatus = $state<ConnectionStatus>('Disconnected');
  
  private saveTimeout: ReturnType<typeof setTimeout> | null = null;
  private unlistenStatus: UnlistenFn | null = null;
  private unlistenCanvaLog: UnlistenFn | null = null;

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
      if (savedConfig.channel.enabled) {
        // We can't easily check if discovery is running without an API, 
        // but we can try to start it if enabled
        await this.startDiscovery();
      }
    } catch (e) {
      console.error('Failed to load config:', e);
      this.configLoaded = true;
    }

    // Listen for status updates
    this.unlistenStatus = await listen<LiveStatus>('presentation-status', (event) => {
      this.liveStatus = event.payload;
    });

    // Listen for Canva webview logs forwarded from Rust
    this.unlistenCanvaLog = await listen<{ category: string; message: string }>('canva-webview-log', (event) => {
      const { category, message } = event.payload;
      console.log(`[CANVA:${category}]`, message);
    });
  }

  destroy() {
    this.unlistenStatus?.();
    this.unlistenCanvaLog?.();
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
    this.scheduleConfigSave();
  }

  async selectPresentation(name: string) {
    if (this.pollingActive) {
      await this.stopPolling();
    }

    this.config = { ...this.config, presentationName: name };
    this.scheduleConfigSave();

    if (name) {
      await this.startPolling();
    }
  }

  updateOscConfig(oscConfig: OscConfig) {
    this.config = { ...this.config, osc: oscConfig };
    this.scheduleConfigSave();
  }

  async updateChannelConfig(channelConfig: ChannelConfig) {
    const wasEnabled = this.config.channel.enabled;
    this.config = { ...this.config, channel: channelConfig };
    this.scheduleConfigSave();

    if (channelConfig.enabled && !wasEnabled && channelConfig.channelName) {
      await this.startDiscovery();
    } else if (!channelConfig.enabled && wasEnabled) {
      await this.stopDiscovery();
    }
  }

  async startDiscovery() {
    if (!this.config.channel.enabled || !this.config.channel.channelName) return;

    try {
      await invoke('start_discovery', {
        instanceId: this.config.channel.instanceId,
        displayName: this.config.channel.displayName,
        channelName: this.config.channel.channelName,
        oscPort: this.config.osc.receivePort,
        networkInterface: this.config.channel.networkInterface
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
      await invoke('next_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
    } catch (e) {
      console.error('Failed to go to next slide:', e);
    }
  }

  async prevSlide() {
    try {
      await invoke('prev_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName
      });
    } catch (e) {
      console.error('Failed to go to previous slide:', e);
    }
  }

  async gotoSlide(slide: number) {
    try {
      await invoke('goto_slide', {
        adapter: this.config.adapter,
        name: this.config.presentationName,
        slide
      });
    } catch (e) {
      console.error('Failed to go to slide:', e);
    }
  }

  updateAdapterConfig(adapterConfig: AdapterConfig) {
    this.config = { ...this.config, adapterConfig };
    this.scheduleConfigSave();
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
}

export const appStore = new AppStore();
