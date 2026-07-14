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
};