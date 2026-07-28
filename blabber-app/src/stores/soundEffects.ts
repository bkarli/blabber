import { defineStore } from 'pinia';
import { api } from '@/lib/tauri';
import { SOUND_EFFECTS, type SoundEffectId } from '@/lib/soundEffects';

const STORAGE_KEY = 'blabber:sound-effects-enabled';

function defaults(): Record<string, boolean> {
  return Object.fromEntries(SOUND_EFFECTS.map((effect) => [effect.id, effect.defaultEnabled]));
}

function loadEnabled(): Record<string, boolean> {
  const merged = defaults();
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) Object.assign(merged, JSON.parse(raw));
  } catch (e) {
    console.warn('failed to load sound effect settings, using defaults', e);
  }
  return merged;
}

export const useSoundEffectsStore = defineStore('soundEffects', {
  state: () => ({
    enabled: loadEnabled(),
  }),

  getters: {
    // catalog joined with live enabled state, for the settings UI
    effects: (state) => SOUND_EFFECTS.map((def) => ({
      ...def,
      enabled: state.enabled[def.id] ?? def.defaultEnabled,
    })),
  },

  actions: {
    setEnabled(id: SoundEffectId, value: boolean) {
      this.enabled[id] = value;
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.enabled));
    },

    /** Plays a registered sound effect if it's enabled. Never throws. */
    async play(id: SoundEffectId) {
      if (this.enabled[id] === false) return;
      const def = SOUND_EFFECTS.find((effect) => effect.id === id);
      if (!def) return;
      try {
        await api.playSoundEffect(def.file);
      } catch (e) {
        console.warn(`sound effect "${id}" failed to play (missing asset file?)`, e);
      }
    },
  },
});
