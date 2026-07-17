import { defineStore } from 'pinia';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export interface SpaceInfo {
  id: string;
  name: string;
}

export interface Message {
  author: string;
  content: string;
  sent_at: number;
}

export interface Member {
  endpoint_id: string;
  display_name: string;
  joined_at: number;
}

export interface RoomInfo {
  id: string;
  name: string;
}

export type AppEvent =
  | { NewMessage: { space_id: string; room_id: string; message: Message } }
  | { NewMember: { space_id: string; member: Member } }
  | { NewRoom: { space_id: string; room_id: string; room_name: string } };

export const useAppStore = defineStore('app', {
  state: () => ({
    spaces: [] as SpaceInfo[],
    roomsBySpace: {} as Record<string, RoomInfo[]>,
    membersBySpace: {} as Record<string, Member[]>,
    messagesByRoom: {} as Record<string, Message[]>,
    initialized: false,
  }),

  getters: {
    roomsFor: (state) => (spaceId: string) => state.roomsBySpace[spaceId] ?? [],
    membersFor: (state) => (spaceId: string) => state.membersBySpace[spaceId] ?? [],
    messagesFor: (state) => (roomId: string) => state.messagesByRoom[roomId] ?? [],
  },

  actions: {
    async init() {
      if (this.initialized) return;
      this.initialized = true;

      try {
        this.spaces = await invoke<SpaceInfo[]>('list_servers');
      } catch (e) {
        console.error('failed to load initial spaces', e);
      }

      await listen<AppEvent>('app-event', (event) => {
        const payload = event.payload;
        if ('NewMessage' in payload) {
          this.handleNewMessage(payload.NewMessage);
        } else if ('NewMember' in payload) {
          this.handleNewMember(payload.NewMember);
        } else if ('NewRoom' in payload) {
          this.handleNewRoom(payload.NewRoom);
        }
      });
    },

    handleNewMessage({ room_id, message }: { space_id: string; room_id: string; message: Message }) {
      const list = (this.messagesByRoom[room_id] ??= []);
      console.log(message.content);
      const alreadyExists = list.some(
        (m) => m.author === message.author && m.sent_at === message.sent_at
      );
      if (!alreadyExists) {
        list.push(message);
        list.sort((a, b) => a.sent_at - b.sent_at);
      }
    },

    handleNewMember({ space_id, member }: { space_id: string; member: Member }) {
      const list = (this.membersBySpace[space_id] ??= []);
      const existingIndex = list.findIndex((m) => m.endpoint_id === member.endpoint_id);
      if (existingIndex >= 0) {
        list[existingIndex] = member;
      } else {
        list.push(member);
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

    async loadRooms(spaceId: string) {
      const rooms = await invoke<RoomInfo[]>('list_rooms', { spaceId });
      this.roomsBySpace[spaceId] = rooms;
    },

    async createRoom(spaceId: string, name: string) {
      const room = await invoke<RoomInfo>('create_room', { spaceId, name });
      const list = (this.roomsBySpace[spaceId] ??= []);
      if (!list.some((r) => r.id === room.id)) {
        list.push(room);
      }
      return room;
    },

    async getInvite(spaceId: string) {
      return invoke<string>('get_invite', { spaceId });
    },
    async loadMessages(spaceId: string, roomId: string) {
      const messages = await invoke<Message[]>('list_messages', { spaceId, roomId });
      this.messagesByRoom[roomId] = messages.sort((a, b) => a.sent_at - b.sent_at);
    },

    async sendMessage(spaceId: string, roomId: string, content: string) {
      await invoke<void>('send_message', { spaceId, roomId, content });
    },
  },
});
