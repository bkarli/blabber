// src/API/tauriAPI.ts

// Später wird dieser Import für die Verbindung zum Rust-Backend benötigt:
// import { invoke } from "@tauri-apps/api/core";

export interface Chat {
    id: number;
    name: string;
    initials: string;
    online: boolean;
}

export interface User {
    id: string;
    username: string;
    initials: string;
}

// Legt fest, welche Funktionen tauriApi anbieten muss.
interface TauriApi {
    login(
        username: string,
        password: string,
    ): Promise<User>;

    logout(): Promise<void>;

    sayHello(): Promise<void>;

    getChats(): Promise<Chat[]>;

    createServer(name: string): Promise<void>;

    sendMessage(
        chatId: number,
        text: string,
    ): Promise<void>;
}

// Momentan sind alle Funktionen nur Mock-Funktionen.
// Später werden die Inhalte durch invoke-Aufrufe ersetzt.
export const tauriApi: TauriApi = {
    async login(
        username: string,
        password: string,
    ): Promise<User> {
        // Kleine Prüfung für das Mock-Login
        if (!username.trim() || !password) {
            throw new Error("Username and password are required.");
        }

        console.log(`Mock: User "${username}" logged in.`);

        // Momentan wird ein Testbenutzer zurückgegeben.
        return {
            id: "test-user-1",
            username: username.trim(),
            initials: createInitials(username),
        };

        // Später wird der Mock-Code ersetzt durch:
        //
        // return await invoke<User>("login", {
        //     username,
        //     password,
        // });
    },

    async logout(): Promise<void> {
        // Momentan wird das Logout nur simuliert.
        console.log("Mock: User logged out.");

        // Später:
        //
        // await invoke<void>("logout");
    },

    async sayHello(): Promise<void> {
        console.log("Hello World from Mock API");

        // Später:
        //
        // const message = await invoke<string>("say_hello");
        // console.log(message);
    },

    async getChats(): Promise<Chat[]> {
        // Momentan werden Test-Chats zurückgegeben.
        return [
            {
                id: 1,
                name: "Programming Group",
                initials: "PG",
                online: true,
            },
            {
                id: 2,
                name: "Cybersecurity",
                initials: "CS",
                online: false,
            },
        ];

        // Später wird die Testliste ersetzt durch:
        //
        // return await invoke<Chat[]>("get_chats");
    },

    async createServer(name: string): Promise<void> {
        if (!name.trim()) {
            throw new Error("Server name cannot be empty.");
        }

        console.log(
            `Mock: Server "${name}" wurde erstellt.`,
        );

        // Später:
        //
        // await invoke<void>("create_server", {
        //     name,
        // });
    },

    async sendMessage(
        chatId: number,
        text: string,
    ): Promise<void> {
        if (!text.trim()) {
            throw new Error("Message cannot be empty.");
        }

        console.log(
            `Mock: Nachricht an Chat ${chatId}: ${text}`,
        );

        // Später:
        //
        // await invoke<void>("send_message", {
        //     chatId,
        //     text,
        // });
    },
};

// Hilfsfunktion für das Mock-Login.
// Sie erstellt zum Beispiel aus "Boran Gökcen" die Initialen "BG".
function createInitials(username: string): string {
    const parts = username
        .trim()
        .split(" ")
        .filter((part) => part.length > 0);

    if (parts.length === 0) {
        return "U";
    }

    if (parts.length === 1) {
        return parts[0]
            .slice(0, 2)
            .toUpperCase();
    }

    return parts
        .slice(0, 2)
        .map((part) => part.charAt(0))
        .join("")
        .toUpperCase();
}