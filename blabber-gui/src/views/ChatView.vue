<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { tauriApi } from "../API/tauriAPI";
import type {
  Chat,
  User,
} from "../API/tauriAPI";

const props = defineProps<{
  user: User;
}>();

const emit = defineEmits<{
  (event: "logout"): void;
  (event: "open-settings"): void;
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
const messageText = ref("");

const isLoading = ref(false);
const isLoggingOut = ref(false);
const isSendingMessage = ref(false);

const loadError = ref("");
const messageError = ref("");

const profileName = computed(() => {
  return props.user.displayName;
});

const profileInitials = computed(() => {
  const parts = props.user.displayName
      .trim()
      .split(/\s+/)
      .filter((part) => part.length > 0);

  if (parts.length === 0) {
    return "U";
  }

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
      chats.value.find(
          (chat) => chat.id === selectedChatId.value,
      ) ?? null
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
  messageText.value = "";
  messageError.value = "";
}

async function loadChats() {
  isLoading.value = true;
  loadError.value = "";

  try {
    const backendChats = await tauriApi.getChats();

    if (backendChats.length > 0) {
      chats.value = backendChats;
      selectedChatId.value = backendChats[0].id;
    } else {
      chats.value = [...defaultChats];
      selectedChatId.value = -1;
    }
  } catch (error) {
    console.error("Could not load chats:", error);

    loadError.value =
        "Backend conversations are currently unavailable.";

    chats.value = [...defaultChats];
    selectedChatId.value = -1;
  } finally {
    isLoading.value = false;
  }
}

async function logout() {
  isLoggingOut.value = true;

  try {
    await tauriApi.logout();
    emit("logout");
  } catch (error) {
    console.error("Logout failed:", error);
  } finally {
    isLoggingOut.value = false;
  }
}
function openSettings(){
  emit("open-settings");
}

async function createServer() {
  const name = window.prompt("Enter a server name:");

  if (!name?.trim()) {
    return;
  }

  try {
    await tauriApi.createServer(name.trim());
  } catch (error) {
    console.error("Could not create server:", error);
  }
}

async function sendCurrentMessage() {
  const chat = selectedChat.value;
  const text = messageText.value.trim();

  if (
      !chat ||
      chat.id === -1 ||
      !text ||
      isSendingMessage.value
  ) {
    return;
  }

  isSendingMessage.value = true;
  messageError.value = "";

  try {
    await tauriApi.sendMessage(chat.id, text);
    messageText.value = "";
  } catch (error) {
    console.error("Could not send message:", error);

    messageError.value =
        "The message could not be sent.";
  } finally {
    isSendingMessage.value = false;
  }
}

onMounted(() => {
  void loadChats();
});
type ServerModalView = "choice" | "join" | "create";
const showServerModal = ref(false);
const serverModalView = ref<ServerModalView>("choice");
const serverTicket = ref("");
const serverName = ref("");

function openServerModal() {
  serverModalView.value = "choice";
  serverTicket.value = "";
  serverName.value = "";
  showServerModal.value = true;
}

function closeServerModal() {
  showServerModal.value = false;
}

function showJoinServer() {
  serverModalView.value = "join";
}

function showCreateServer() {
  serverModalView.value = "create";
}

function returnToServerChoice() {
  serverModalView.value = "choice";}

</script>

<template>
  <div class="app">
    <!-- Server navigation -->
    <aside class="server-sidebar">
      <button
          class="server-button home-server"
          title="Blabber home"
      >
        B
      </button>

      <div class="server-divider"></div>

      <button
          class="server-button active-server"
          title="Programming Group"
      >
        PG
      </button>

      <button
          class="server-button"
          title="Cybersecurity"
      >
        CS
      </button>

      <button
          class="server-button add-server"
          title="Create server"
          @click="createServer"
      >
        +
      </button>

      <button
          class="server-button settings-button"
          title="SettingsAlloAllo"
          @click = "openSettings"
      >
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
            class="new-conversation-button"
            type="button"
            title="Add server"
            @click="openServerModal"
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
            :class="{
            selected: selectedChatId === chat.id,
          }"
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
            v-if="
            !isLoading &&
            filteredChats.length === 0
          "
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
            :disabled="isLoggingOut"
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
                {{
                  selectedChat.online
                      ? "Online"
                      : "Offline"
                }}
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
            Select or create a conversation to start
            chatting.
          </p>

          <p v-else>
            No messages yet.
          </p>
        </section>

        <p
            v-if="messageError"
            class="message-error"
        >
          {{ messageError }}
        </p>

        <footer
            v-if="selectedChat.id !== -1"
            class="message-bar"
        >
          <button
              class="attachment-button"
              type="button"
              title="Add attachment"
          >
            +
          </button>

          <input
              v-model="messageText"
              type="text"
              :disabled="isSendingMessage"
              :placeholder="`Message ${selectedChat.name}`"
              @keyup.enter="sendCurrentMessage"
          />

          <button
              class="send-button"
              type="button"
              :disabled="
              !messageText.trim() ||
              isSendingMessage
            "
              @click="sendCurrentMessage"
          >
            {{
              isSendingMessage
                  ? "Sending..."
                  : "Send"
            }}
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
  <Teleport to="body">
    <div
        v-if="showServerModal"
        class="modal-backdrop"
        @click.self="closeServerModal"
    >
      <section class="server-modal">
        <button
            class="modal-close-button"
            type="button"
            @click="closeServerModal"
        >
          ×
        </button>

        <template v-if="serverModalView === 'choice'">
          <h2>Add a server</h2>

          <p class="modal-description">
            Create a new server or join an existing one.
          </p>

          <div class="server-choice-list">
            <button
                class="server-choice-button"
                type="button"
                @click="showCreateServer"
            >
              Create Server
            </button>

            <button
                class="server-choice-button"
                type="button"
                @click="showJoinServer"
            >
              Join Server
            </button>
          </div>
        </template>

        <template v-else-if="serverModalView === 'join'">
          <button
              class="modal-back-button"
              type="button"
              @click="returnToServerChoice"
          >
            ← Back
          </button>

          <h2>Join a server</h2>

          <p class="modal-description">
            Enter the server ticket.
          </p>

          <label for="server-ticket">
            Server ticket
          </label>

          <input
              id="server-ticket"
              v-model="serverTicket"
              type="text"
              placeholder="Enter server ticket"
          />

          <button
              class="primary-modal-button"
              type="button"
          >
            Join Server
          </button>
        </template>

        <template v-else>
          <button
              class="modal-back-button"
              type="button"
              @click="returnToServerChoice"
          >
            ← Back
          </button>

          <h2>Create a server</h2>

          <p class="modal-description">
            Enter a name for the new server.
          </p>

          <label for="server-name">
            Server name
          </label>

          <input
              id="server-name"
              v-model="serverName"
              type="text"
              placeholder="Enter server name"
          />

          <button
              class="primary-modal-button"
              type="button"
          >
            Create Server
          </button>
        </template>
      </section>
    </div>
  </Teleport>
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

button:disabled {
  cursor: not-allowed;
}

.app {
  display: grid;
  grid-template-columns:
    72px 290px minmax(0, 1fr);
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
  background: linear-gradient(
      145deg,
      #5865f2,
      #8b5cf6
  );
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

.logout-button:hover:not(:disabled) {
  color: white;
  background: #30333c;
}

.logout-button:disabled {
  opacity: 0.5;
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

.message-error {
  margin: 0 22px 10px;
  color: #f0b8b8;
  font-size: 13px;
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
  color: white;
  background: transparent;
}

.message-bar input::placeholder {
  color: #9499a6;
}

.message-bar button {
  padding: 9px 14px;
  border-radius: 8px;
  background: #414550;
}

.attachment-button {
  color: #b5bac1;
}

.send-button {
  color: white;
  background: #5865f2 !important;
}

.send-button:hover:not(:disabled) {
  background: #4752c4 !important;
}

.send-button:disabled {
  opacity: 0.5;
}
.modal-backdrop {
  position: fixed;
  z-index: 1000;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 24px;
  background: rgba(6, 7, 10, 0.72);
}

.server-modal {
  position: relative;
  width: min(420px, 100%);
  padding: 32px;
  border: 1px solid #353844;
  border-radius: 18px;
  color: #f2f3f5;
  background: #1d1f26;
  box-shadow: 0 30px 90px rgba(0, 0, 0, 0.5);
}

.server-modal h2 {
  margin: 0;
  text-align: center;
}

.modal-description {
  margin: 10px 0 24px;
  color: #9499a6;
  text-align: center;
}

.modal-close-button {
  position: absolute;
  top: 12px;
  right: 16px;
  border: none;
  color: #9499a6;
  background: transparent;
  font-size: 28px;
  cursor: pointer;
}

.modal-close-button:hover {
  color: white;
}

.server-choice-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.server-choice-button {
  height: 48px;
  border: none;
  border-radius: 10px;
  color: white;
  background: #5865f2;
  font-weight: 700;
  cursor: pointer;
}

.server-choice-button:hover {
  background: #4752c4;
}

.modal-back-button {
  margin-bottom: 18px;
  padding: 0;
  border: none;
  color: #b5bac1;
  background: transparent;
  cursor: pointer;
}

.server-modal label {
  display: block;
  margin-bottom: 8px;
  color: #b5bac1;
  font-size: 13px;
  font-weight: 600;
}

.server-modal input {
  width: 100%;
  height: 48px;
  padding: 0 14px;
  border: 1px solid transparent;
  border-radius: 10px;
  outline: none;
  color: white;
  background: #121419;
}

.server-modal input:focus {
  border-color: #5865f2;
}

.primary-modal-button {
  width: 100%;
  height: 46px;
  margin-top: 18px;
  border: none;
  border-radius: 10px;
  color: white;
  background: #5865f2;
  font-weight: 700;
  cursor: pointer;
}

.primary-modal-button:hover {
  background: #4752c4;
}

@media (max-width: 800px) {
  .app {
    grid-template-columns:
      64px 230px minmax(0, 1fr);
  }
}
</style>