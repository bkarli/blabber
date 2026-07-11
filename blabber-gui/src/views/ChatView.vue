<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

interface Chat {
  id: number;
  name: string;
  initials: string;
  online: boolean;
}

const props = defineProps<{
  username: string;
}>();

const emit = defineEmits<{
  (event: "logout"): void;
}>();

const defaultChats: Chat[] = [
  {
    id: -1,
    name: "Welcome to Blabber",
    initials: "B",
    online: false,
  },
];

const chats = ref<Chat[]>([...defaultChats]);
const selectedChatId = ref<number | null>(-1);
const searchQuery = ref("");

const isLoading = ref(false);
const loadError = ref("");

const profileName = computed(() => {
  return props.username.trim() || "User";
});

const profileInitials = computed(() => {
  const parts = profileName.value
      .split(" ")
      .filter((part) => part.length > 0);

  if (parts.length === 1) {
    return parts[0].slice(0, 2).toUpperCase();
  }

  return parts
      .slice(0, 2)
      .map((part) => part.charAt(0))
      .join("")
      .toUpperCase();
});

const selectedChat = computed(() => {
  return (
      chats.value.find((chat) => chat.id === selectedChatId.value) ?? null
  );
});

const filteredChats = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();

  if (!query) {
    return chats.value;
  }

  return chats.value.filter((chat) =>
      chat.name.toLowerCase().includes(query),
  );
});

function openChat(chatId: number) {
  selectedChatId.value = chatId;
}

function logout() {
  emit("logout");
}

async function loadChats() {
  isLoading.value = true;
  loadError.value = "";

  try {
    const backendChats = await invoke<Chat[]>("get_chats");

    if (backendChats.length > 0) {
      chats.value = backendChats;
      selectedChatId.value = backendChats[0].id;
    }
  } catch (error) {
    console.error("Could not load chats:", error);

    loadError.value =
        "Backend conversations are currently unavailable.";
  } finally {
    isLoading.value = false;
  }
}

onMounted(() => {
  loadChats();
});
</script>

<template>
  <div class="app">
    <!-- Server navigation -->
    <aside class="server-sidebar">
      <button class="server-button home-server">
        B
      </button>

      <div class="server-divider"></div>

      <button class="server-button active-server">
        PG
      </button>

      <button class="server-button">
        CS
      </button>

      <button class="server-button add-server">
        +
      </button>

      <button class="server-button settings-button">
        ⚙
      </button>
    </aside>

    <!-- Conversation sidebar -->
    <aside class="chat-sidebar">
      <header class="sidebar-header">
        <div>
          <h1>Blabber</h1>
          <span>Direct messages</span>
        </div>

        <button
            class="new-chat-button"
            title="New conversation"
        >
          +
        </button>
      </header>

      <div class="search-container">
        <input
            v-model="searchQuery"
            type="text"
            placeholder="Search conversations"
        />
      </div>

      <nav class="chat-list">
        <p
            v-if="isLoading"
            class="backend-status"
        >
          Loading conversations...
        </p>

        <p
            v-else-if="loadError"
            class="backend-status warning"
        >
          {{ loadError }}
        </p>

        <button
            v-for="chat in filteredChats"
            :key="chat.id"
            class="chat-item"
            :class="{ selected: selectedChatId === chat.id }"
            @click="openChat(chat.id)"
        >
          <div class="avatar">
            {{ chat.initials }}

            <span
                class="status"
                :class="{ online: chat.online }"
            ></span>
          </div>

          <div class="chat-information">
            <strong>{{ chat.name }}</strong>

            <span>
              {{ chat.online ? "Online" : "Offline" }}
            </span>
          </div>
        </button>

        <p
            v-if="filteredChats.length === 0"
            class="no-results"
        >
          No conversations found.
        </p>
      </nav>

      <footer class="profile">
        <div class="profile-avatar">
          {{ profileInitials }}
        </div>

        <div class="profile-information">
          <strong>{{ profileName }}</strong>
          <span>Online</span>
        </div>

        <button
            class="logout-button"
            title="Log out"
            @click="logout"
        >
          ↪
        </button>
      </footer>
    </aside>

    <!-- Main chat area -->
    <main class="chat-area">
      <template v-if="selectedChat">
        <header class="chat-header">
          <div class="chat-user">
            <div class="header-avatar">
              {{ selectedChat.initials }}
            </div>

            <div>
              <h2>{{ selectedChat.name }}</h2>

              <span>
                {{ selectedChat.online ? "Online" : "Offline" }}
              </span>
            </div>
          </div>

          <div class="chat-actions">
            <button title="Voice call">
              ☎
            </button>

            <button title="Video call">
              ▣
            </button>

            <button title="Conversation information">
              ⓘ
            </button>
          </div>
        </header>

        <section class="empty-chat">
          <div class="large-avatar">
            {{ selectedChat.initials }}
          </div>

          <h2>{{ selectedChat.name }}</h2>

          <p v-if="selectedChat.id === -1">
            Select or create a conversation to start chatting.
          </p>

          <p v-else>
            No messages yet.
          </p>
        </section>

        <footer
            v-if="selectedChat.id !== -1"
            class="message-bar"
        >
          <button disabled>
            +
          </button>

          <input
              disabled
              type="text"
              :placeholder="`Message ${selectedChat.name}`"
          />

          <button disabled>
            Send
          </button>
        </footer>
      </template>

      <section
          v-else
          class="no-chat-selected"
      >
        <div class="empty-icon">
          B
        </div>

        <h2>Select a conversation</h2>

        <p>
          Choose a conversation from the list to open it.
        </p>
      </section>
    </main>
  </div>
</template>

<style scoped>
:global(*) {
  box-sizing: border-box;
}

:global(html),
:global(body),
:global(#app) {
  width: 100%;
  height: 100%;
  margin: 0;
}

:global(body) {
  overflow: hidden;
  font-family:
      Inter,
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      sans-serif;
  background: #111318;
}

button,
input {
  font: inherit;
}

button {
  border: none;
  color: inherit;
  cursor: pointer;
}

.app {
  display: grid;
  grid-template-columns: 72px 290px minmax(0, 1fr);
  width: 100vw;
  height: 100vh;
  color: #f2f3f5;
}

/* Server navigation */

.server-sidebar {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 11px;
  padding: 14px 0;
  background: #17191f;
  border-right: 1px solid #292c34;
}

.server-button {
  width: 48px;
  height: 48px;
  border-radius: 16px;
  background: #2b2e36;
  font-weight: 700;
  transition:
      background 150ms ease,
      border-radius 150ms ease;
}

.server-button:hover,
.active-server {
  border-radius: 13px;
  background: #5865f2;
}

.home-server {
  background: #5865f2;
}

.server-divider {
  width: 34px;
  height: 1px;
  background: #343740;
}

.add-server {
  color: #23a55a;
}

.settings-button {
  margin-top: auto;
}

/* Conversation sidebar */

.chat-sidebar {
  display: flex;
  min-width: 0;
  flex-direction: column;
  background: #1d1f26;
  border-right: 1px solid #292c34;
}

.sidebar-header {
  display: flex;
  min-height: 76px;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
  border-bottom: 1px solid #292c34;
}

.sidebar-header h1 {
  margin: 0;
  font-size: 20px;
}

.sidebar-header span {
  color: #9499a6;
  font-size: 12px;
}

.new-chat-button {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: #2b2e36;
  font-size: 22px;
}

.new-chat-button:hover {
  background: #383c46;
}

.search-container {
  padding: 14px;
}

.search-container input {
  width: 100%;
  height: 40px;
  padding: 0 13px;
  border: 1px solid transparent;
  border-radius: 9px;
  outline: none;
  color: white;
  background: #121419;
}

.search-container input:focus {
  border-color: #5865f2;
}

.chat-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 9px;
}

.backend-status {
  margin: 4px 6px 12px;
  padding: 9px;
  border-radius: 8px;
  color: #b5bac1;
  background: #272a32;
  font-size: 12px;
  text-align: center;
}

.backend-status.warning {
  color: #f0b8b8;
  background: rgba(237, 66, 69, 0.12);
}

.chat-item {
  display: flex;
  width: 100%;
  align-items: center;
  gap: 12px;
  padding: 10px;
  margin-bottom: 4px;
  border-radius: 10px;
  text-align: left;
  background: transparent;
}

.chat-item:hover {
  background: #272a32;
}

.chat-item.selected {
  background: #353943;
}

.avatar,
.header-avatar,
.large-avatar,
.profile-avatar,
.empty-icon {
  display: grid;
  flex-shrink: 0;
  place-items: center;
  border-radius: 50%;
  font-weight: 700;
  background: linear-gradient(145deg, #5865f2, #8b5cf6);
}

.avatar {
  position: relative;
  width: 44px;
  height: 44px;
}

.status {
  position: absolute;
  right: -1px;
  bottom: -1px;
  width: 13px;
  height: 13px;
  border: 3px solid #1d1f26;
  border-radius: 50%;
  background: #747982;
}

.status.online {
  background: #23a55a;
}

.chat-information {
  display: flex;
  min-width: 0;
  flex-direction: column;
}

.chat-information strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-information span {
  margin-top: 3px;
  color: #9499a6;
  font-size: 12px;
}

.no-results {
  padding: 20px;
  color: #9499a6;
  text-align: center;
}

.profile {
  display: flex;
  min-height: 64px;
  align-items: center;
  gap: 10px;
  padding: 10px;
  background: #17191f;
}

.profile-avatar {
  width: 38px;
  height: 38px;
}

.profile-information {
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
}

.profile-information strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-information span {
  color: #23a55a;
  font-size: 12px;
}

.logout-button {
  width: 34px;
  height: 34px;
  padding: 7px;
  border-radius: 7px;
  color: #b5bac1;
  background: transparent;
  font-size: 18px;
}

.logout-button:hover {
  color: white;
  background: #30333c;
}

/* Main chat area */

.chat-area {
  display: flex;
  min-width: 0;
  flex-direction: column;
  background: #23262d;
}

.chat-header {
  display: flex;
  min-height: 76px;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
  border-bottom: 1px solid #30333b;
}

.chat-user {
  display: flex;
  align-items: center;
  gap: 12px;
}

.header-avatar {
  width: 42px;
  height: 42px;
}

.chat-user h2 {
  margin: 0;
  font-size: 17px;
}

.chat-user span {
  color: #9499a6;
  font-size: 12px;
}

.chat-actions {
  display: flex;
  gap: 8px;
}

.chat-actions button {
  width: 38px;
  height: 38px;
  border-radius: 9px;
  color: #b5bac1;
  background: transparent;
}

.chat-actions button:hover {
  color: white;
  background: #30343d;
}

.empty-chat,
.no-chat-selected {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  padding: 30px;
  text-align: center;
}

.large-avatar {
  width: 80px;
  height: 80px;
  font-size: 25px;
}

.empty-chat h2,
.no-chat-selected h2 {
  margin: 18px 0 6px;
}

.empty-chat p,
.no-chat-selected p {
  margin: 0;
  color: #9499a6;
}

.empty-icon {
  width: 90px;
  height: 90px;
  font-size: 34px;
}

.message-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 22px 22px;
  padding: 9px 10px;
  border-radius: 12px;
  background: #31343d;
}

.message-bar input {
  min-width: 0;
  height: 40px;
  flex: 1;
  border: none;
  outline: none;
  color: #9499a6;
  background: transparent;
}

.message-bar button {
  padding: 9px 14px;
  border-radius: 8px;
  background: #414550;
  opacity: 0.6;
  cursor: not-allowed;
}

@media (max-width: 800px) {
  .app {
    grid-template-columns: 64px 230px minmax(0, 1fr);
  }
}
</style>