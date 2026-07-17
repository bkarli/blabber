import { invoke } from "@tauri-apps/api/core";

export interface Chat {
    id: number;
    name: string;
    initials: string;
    online: boolean;
}

export interface User {
    displayName: string;
}

export interface SpaceInfo{
    id: string;
    name: string;
}

export interface RoomInfo {
    id: string;
    name: string;
}

export const tauriApi = {
    async login(
        displayName: string,
        password: string,
    ): Promise<User> {
        const returnedDisplayName = await invoke<string>(
            "login",
            {
                displayName,
                password,
            },
        );

        return {
            displayName: returnedDisplayName,
        };
    },

    async createIdentity(
        displayName: string,
        password: string,
    ): Promise<User> {
        const returnedDisplayName = await invoke<string>(
            "create_identity",
            {
                displayName,
                password,
            },
        );

        return {
            displayName: returnedDisplayName,
        };
    },

    async listIdentities(): Promise<string[]> {
        return await invoke<string[]>("list_identities");
    },
    async getChats(): Promise<Chat[]> {
        return [];
    },

    async logout(): Promise<void> {
        await invoke<void>("logout");

    },

    async createServer(name: string): Promise<SpaceInfo> {
        return await invoke<SpaceInfo>(
            "create_server",
            {
                name,
            },
        );
    },

    async listServers(): Promise<SpaceInfo[]> {
        return await invoke<SpaceInfo[]>("list_servers");
    },
    async getInvite(spaceId: string): Promise<string> {
        return await invoke<string>(
            "get_invite",
            {
                spaceId,
            },
        );
    },
    async joinSpace(ticket: string): Promise<SpaceInfo> {
        return await invoke<SpaceInfo>("join_space", {
            ticket,
        });
    },
    async listRooms(
        spaceId: string,
    ): Promise<RoomInfo[]> {
        return await invoke<RoomInfo[]>(
            "list_rooms",
            {
                spaceId,
            },
        );
    },

    async createRoom(
        spaceId: string,
        name: string,
    ): Promise<RoomInfo> {
        return await invoke<RoomInfo>(
            "create_room",
            {
                spaceId,
                name,
            },
        );
    },

    async startCall(peerEndpointId: string): Promise<void>{
        await invoke<void>("start_call",{peerEndpointId,
            },
        );
    },

    async hangUp(): Promise<void>{
        await invoke<void>("hang_up");
    },
    async sendMessage(
        chatId: number,
        text: string,
    ): Promise<void> {
        console.log(
            "Send message not implemented yet:",
            chatId,
            text,
        );
    }
};
