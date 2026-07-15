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
        return;
    },

    async createServer(name: string): Promise<SpaceInfo> {
        return await invoke<SpaceInfo>(
            "create_server",
            {
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