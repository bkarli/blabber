// lib/tauri.ts
import { invoke } from '@tauri-apps/api/core';
import type { SpaceInfo } from '@/stores/app';

export interface SpaceInfo {
  id: string;
  name: string;
}


export interface RoomInfo {
  id: string;
  name: string;
}





export const api = {
  createIdentity: (displayName: string, password: string) =>
    invoke<string>('create_identity', { displayName, password }),

  login: (displayName: string, password: string) =>
    invoke<string>('login', { displayName, password }),

  logout: () => invoke<void>('logout'),

  myEndpointId: () => invoke<string>('my_endpoint_id'),
  deleteIdentity: (displayName: string) => invoke<void>('delete_identity', { displayName }),

  listIdentities: () => invoke<string[]>('list_identities'),

  createServer: (name: string) => invoke<SpaceInfo>('create_server', { name }),
  listServers: () => invoke<SpaceInfo[]>('list_servers'),
  getInvite: (spaceId: string) => invoke<string>('get_invite', { spaceId }),
  joinSpace: (ticket: string) => invoke<SpaceInfo>('join_space', { ticket }),

  listRooms: (spaceId: string) => invoke<RoomInfo[]>('list_rooms', { spaceId }),
  createRoom: (spaceId: string, name: string) => invoke<RoomInfo>('create_room', { spaceId, name }),
};
