import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  type AppConfig,
  type LiveStatus,
  type AdapterType,
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
  
  private saveTimeout: ReturnType<typeof setTimeout> | null = null;
  private unlistenStatus: UnlistenFn | null = null;

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
  }

  destroy() {
    this.unlistenStatus?.();
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
}

export const appStore = new AppStore();
