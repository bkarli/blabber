import { defineStore } from 'pinia';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { useSoundEffectsStore } from '@/stores/soundEffects';

export interface SpaceInfo {
  id: string;
  name: string;
}

export interface Member {
  author_id: string;
  endpoint_id: string;
  display_name: string;
  joined_at: number;
}

export interface RoomInfo {
  id: string;
  name: string;
}

export interface ChannelInfo {
  id: string;
  name: string;
}

export type MessageContent =
  | { Text: { text: string } }
  | { Image: { filename: string; mime: string; thumbnail_base64: string; media_key: string } }
  | { File: { filename: string; mime: string; size: number; media_key: string } };

export interface Message {
  author: string;
  content: MessageContent;
  sent_at: number;
}

export type AppEvent =
  | { type: 'NewMessage'; space_id: string; room_id: string; message: Message }
  | { type: 'NewMember'; space_id: string; member: Member }
  | { type: 'MemberLeft'; space_id: string; author_id: string }
  | { type: 'NewRoom'; space_id: string; room_id: string; room_name: string }
  | { type: 'NewCallRoom'; space_id: string; room_id: string; room_name: string }
  | { type: 'NewCallParticipant'; space_id: string; room_id: string; endpoint_id: string }
  | { type: 'CallParticipantLeft'; space_id: string; room_id: string; endpoint_id: string };

export const useAppStore = defineStore('app', {
  state: () => ({
    spaces: [] as SpaceInfo[],
    myAuthorId: null as string | null,
    myEndpointId: null as string | null,
    roomsBySpace: {} as Record<string, RoomInfo[]>,
    channelsBySpace: {} as Record<string, ChannelInfo[]>,
    membersBySpace: {} as Record<string, Member[]>,
    messagesByRoom: {} as Record<string, Message[]>,
    activeRoomId: null as string | null,
    unreadRoomIds: {} as Record<string, boolean>,
    activeCallRoomId: null as string | null,
    callParticipants: [] as string[],
    isMuted: false,
    initialized: false,
  }),

  getters: {
    roomsFor: (state) => (spaceId: string) => state.roomsBySpace[spaceId] ?? [],
    channelsFor: (state) => (spaceId: string) => state.channelsBySpace[spaceId] ?? [],
    membersFor: (state) => (spaceId: string) => state.membersBySpace[spaceId] ?? [],
    messagesFor: (state) => (roomId: string) => state.messagesByRoom[roomId] ?? [],
    isRoomUnread: (state) => (roomId: string) => !!state.unreadRoomIds[roomId],

    displayNameFor: (state) => (spaceId: string, authorId: string) => {
      if (authorId === state.myAuthorId) return 'You';
      const member = (state.membersBySpace[spaceId] ?? []).find((m) => m.author_id === authorId);
      return member?.display_name ?? authorId.slice(0, 8);
    },

    displayNameForEndpoint: (state) => (spaceId: string, endpointId: string) => {
      const member = (state.membersBySpace[spaceId] ?? []).find((m) => m.endpoint_id === endpointId);
      return member?.display_name ?? endpointId.slice(0, 8);
    },
  },

  actions: {
    async init() {
      if (this.initialized) return;
      this.initialized = true;

      try {
        this.myAuthorId = await invoke<string>('get_my_author_id');
      } catch (e) {
        console.error('failed to load own author id', e);
      }

      try {
        this.myEndpointId = await invoke<string>('my_endpoint_id');
      } catch (e) {
        console.error('failed to load own endpoint id', e);
      }

      try {
        this.spaces = await invoke<SpaceInfo[]>('list_servers');
      } catch (e) {
        console.error('failed to load initial spaces', e);
      }

      await listen<AppEvent>('app-event', (event) => {
        const payload = event.payload;

        if (payload.type === 'NewMessage') {
          this.handleNewMessage(payload);
        } else if (payload.type === 'NewMember') {
          this.handleNewMember(payload);
        } else if (payload.type === 'MemberLeft') {
          this.handleMemberLeft(payload);
        } else if (payload.type === 'NewRoom') {
          this.handleNewRoom(payload);
        } else if (payload.type === 'NewCallRoom') {
          this.handleNewCallRoom(payload);
        } else if (payload.type === 'NewCallParticipant') {
          this.handleNewCallParticipant(payload);
        } else if (payload.type === 'CallParticipantLeft') {
          this.handleCallParticipantLeft(payload);
        }
      });
    },

    handleNewCallRoom({ space_id, room_id, room_name }: { space_id: string; room_id: string; room_name: string }) {
      const list = (this.channelsBySpace[space_id] ??= []);
      const alreadyExists = list.some((c) => c.id === room_id);
      if (!alreadyExists) {
        list.push({ id: room_id, name: room_name });
      }
    },

    handleNewCallParticipant({ room_id, endpoint_id }: { space_id: string; room_id: string; endpoint_id: string }) {
      if (room_id === this.activeCallRoomId) {
        if (!this.callParticipants.includes(endpoint_id)) {
          this.callParticipants.push(endpoint_id);
        }
      }
    },

    handleCallParticipantLeft({ room_id, endpoint_id }: { space_id: string; room_id: string; endpoint_id: string }) {
      if (room_id === this.activeCallRoomId) {
        this.callParticipants = this.callParticipants.filter((id) => id !== endpoint_id);
        // our own leave already plays the sound from leaveCallRoom() directly
        if (endpoint_id !== this.myEndpointId) {
          useSoundEffectsStore().play('call-leave');
        }
      }
    },

    handleNewMessage({ room_id, message }: { space_id: string; room_id: string; message: Message }) {
      const list = (this.messagesByRoom[room_id] ??= []);
      const alreadyExists = list.some(
        (m) => m.author === message.author && m.sent_at === message.sent_at
      );
      if (!alreadyExists) {
        list.push(message);
        list.sort((a, b) => a.sent_at - b.sent_at);
        // our own message already played 'message-send' from sendMessage() directly
        if (message.author !== this.myAuthorId) {
          useSoundEffectsStore().play('message-receive');
          if (room_id !== this.activeRoomId) {
            this.unreadRoomIds[room_id] = true;
          }
        }
      }
    },

    /** Call when the user opens (or navigates away from) a room, so the unread dot tracks what's actually been seen. */
    setActiveRoom(roomId: string | null) {
      this.activeRoomId = roomId;
      if (roomId) {
        delete this.unreadRoomIds[roomId];
      }
    },

    handleNewMember({ space_id, member }: { space_id: string; member: Member }) {
      const list = (this.membersBySpace[space_id] ??= []);
      const existingIndex = list.findIndex((m) => m.author_id === member.author_id);
      if (existingIndex >= 0) {
        list[existingIndex] = member;
      } else {
        list.push(member);
      }
    },

    handleMemberLeft({ space_id, author_id }: { space_id: string; author_id: string }) {
      const list = this.membersBySpace[space_id];
      if (list) {
        this.membersBySpace[space_id] = list.filter((m) => m.author_id !== author_id);
      }
    },

    handleNewRoom({ space_id, room_id, room_name }: { space_id: string; room_id: string; room_name: string }) {
      const list = (this.roomsBySpace[space_id] ??= []);
      const alreadyExists = list.some((r) => r.id === room_id);
      if (!alreadyExists) {
        list.push({ id: room_id, name: room_name });
      }
    },

    async createSpace(name: string) {
      const info = await invoke<SpaceInfo>('create_server', { name });
      const alreadyExists = this.spaces.some((s) => s.id === info.id);
      if (!alreadyExists) {
        this.spaces.push(info);
      }
      return info;
    },

    async joinSpace(ticket: string) {
      const info = await invoke<SpaceInfo>('join_space', { ticket });
      const alreadyExists = this.spaces.some((s) => s.id === info.id);
      if (!alreadyExists) {
        this.spaces.push(info);
      }
      return info;
    },

    async leaveSpace(spaceId: string) {
      await invoke<void>('leave_space', { spaceId });
      this.spaces = this.spaces.filter((s) => s.id !== spaceId);
      delete this.roomsBySpace[spaceId];
      delete this.channelsBySpace[spaceId];
      delete this.membersBySpace[spaceId];
    },

    async loadRooms(spaceId: string) {
      const rooms = await invoke<RoomInfo[]>('list_rooms', { spaceId });
      // merge rather than replace: a NewRoom event from the live watcher can
      // land while this fetch is still in flight, and a stale/incomplete
      // snapshot resolving afterwards must not erase a room that was just
      // correctly added - same race as loadMembers below.
      const merged = new Map((this.roomsBySpace[spaceId] ?? []).map((r) => [r.id, r]));
      for (const room of rooms) {
        merged.set(room.id, room);
      }
      this.roomsBySpace[spaceId] = Array.from(merged.values());
    },

    async loadChannels(spaceId: string) {
      try {
        const channels = await invoke<ChannelInfo[]>('list_call_rooms', { spaceId });
        const merged = new Map((this.channelsBySpace[spaceId] ?? []).map((c) => [c.id, c]));
        for (const channel of channels) {
          merged.set(channel.id, channel);
        }
        this.channelsBySpace[spaceId] = Array.from(merged.values());
      } catch (e) {
        console.log('failed to load channels', e);
      }
    },

    async createRoom(spaceId: string, name: string) {
      const room = await invoke<RoomInfo>('create_room', { spaceId, name });
      const list = (this.roomsBySpace[spaceId] ??= []);
      if (!list.some((r) => r.id === room.id)) {
        list.push(room);
      }
      return room;
    },

    async createChannel(spaceId: string, name: string) {
      const channel = await invoke<RoomInfo>('create_call_room', { spaceId, name });
      const list = (this.channelsBySpace[spaceId] ??= []);
      if (!list.some((r) => r.id === channel.id)) {
        list.push(channel);
      }
      return channel;
    },

    async getInvite(spaceId: string) {
      return invoke<string>('get_invite', { spaceId });
    },

    async loadMessages(spaceId: string, roomId: string) {
      const messages = await invoke<Message[]>('list_messages', { spaceId, roomId });
      // merge rather than replace: list_messages can skip a message whose
      // content blob hasn't finished syncing locally yet (blabber-core's
      // Room::list_messages), and a NewMessage event can also land while
      // this fetch is still in flight - same race as loadMembers/loadRooms.
      // Keyed the same way handleNewMessage's own dedup check is.
      const key = (m: Message) => `${m.author}:${m.sent_at}`;
      const merged = new Map((this.messagesByRoom[roomId] ?? []).map((m) => [key(m), m]));
      for (const message of messages) {
        merged.set(key(message), message);
      }
      this.messagesByRoom[roomId] = Array.from(merged.values()).sort((a, b) => a.sent_at - b.sent_at);
    },

    async loadFullImage(spaceId: string, roomId: string, messageKey: string) {
      return invoke<Message>('get_exact_message', { spaceId, roomId, messageKey });
    },

    async sendMessage(spaceId: string, roomId: string, content: string) {
      await invoke<void>('send_message', { spaceId, roomId, content });
      useSoundEffectsStore().play('message-send');
    },

    async sendImage(spaceId: string, roomId: string, path: string) {
      await invoke<void>('send_image', { spaceId, roomId, path });
      useSoundEffectsStore().play('message-send');
    },

    async sendFile(spaceId: string, roomId: string, path: string) {
      await invoke<void>('send_file', { spaceId, roomId, path });
    },

    async getMedia(spaceId: string, roomId: string, mediaKey: string) {
      return invoke<string | null>('get_media', { spaceId, roomId, mediaKey });
    },

    async joinCallRoom(roomId: string) {
      await invoke<void>('join_call_room', { roomId });
      // reset first (clears any stale participants from a previous room),
      // then set activeCallRoomId so NewCallParticipant events for this
      // room start accumulating into a clean array before the fetch below
      // resolves.
      this.callParticipants = [];
      this.activeCallRoomId = roomId;
      const participants = await invoke<string[]>('list_call_participants', { roomId });
      // merge rather than replace: someone else's NewCallParticipant event
      // can land while this fetch is in flight, and a stale snapshot
      // resolving afterwards must not erase them - same race as
      // loadMembers/loadRooms/loadMessages. Removal stays exclusively
      // handled by handleCallParticipantLeft.
      this.callParticipants = Array.from(new Set([...this.callParticipants, ...participants]));
      useSoundEffectsStore().play('call-join');
    },

    async leaveCallRoom() {
      await invoke<void>('leave_call_room');
      this.activeCallRoomId = null;
      this.callParticipants = [];
      this.isMuted = false;
      useSoundEffectsStore().play('call-leave');
    },

    async setMuted(muted: boolean) {
      await invoke<void>('set_muted', { muted });
      this.isMuted = muted;
      useSoundEffectsStore().play(muted ? 'mute' : 'unmute');
    },

    async loadCallParticipants(roomId: string) {
      return invoke<string[]>('list_call_participants', { roomId });
    },

    async loadMembers(spaceId: string) {
      const members = await invoke<Member[]>('list_members', { spaceId });
      const merged = new Map(
        (this.membersBySpace[spaceId] ?? []).map((m) => [m.author_id, m])
      );
      for (const member of members) {
        merged.set(member.author_id, member);
      }
      this.membersBySpace[spaceId] = Array.from(merged.values());
    },
  },
});
