// Central catalog of toggleable sound effects. To add a new one:
//   1. Drop `<file>.mp3` into blabber-app/src-tauri/assets/sounds/
//   2. Add an entry below
//   3. Call `useSoundEffectsStore().play('<id>')` at the trigger site
// Everything else picks it up automatically.
export interface SoundEffectDef {
  id: string;
  label: string;
  /** Matches src-tauri/assets/sounds/<file>.mp3, passed to the `play_sound_effect` command. */
  file: string;
  defaultEnabled: boolean;
}

export const SOUND_EFFECTS: SoundEffectDef[] = [
  {
    id: 'message-send',
    label: 'Sending a message',
    file: 'message-send',
    defaultEnabled: true,
  },
  {
    id: 'message-receive',
    label: 'Receiving a message',
    file: 'message-receive',
    defaultEnabled: true,
  },
  {
    id: 'call-join',
    label: 'Joining a voice channel',
    file: 'call-join',
    defaultEnabled: true,
  },
  {
    id: 'call-leave',
    label: 'Leaving a voice channel',
    file: 'call-leave',
    defaultEnabled: true,
  },
  {
    id: 'mute',
    label: 'Muting your mic',
    file: 'mute',
    defaultEnabled: true,
  },
  {
    id: 'unmute',
    label: 'Unmuting your mic',
    file: 'unmute',
    defaultEnabled: true,
  },
];

export type SoundEffectId = (typeof SOUND_EFFECTS)[number]['id'];
