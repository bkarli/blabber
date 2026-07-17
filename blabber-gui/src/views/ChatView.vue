<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { tauriApi } from "../API/tauriAPI";
import type {
  Chat,
  User,
  SpaceInfo,
  RoomInfo,
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
const spaces = ref<SpaceInfo[]>([]);
const selectedSpaceId = ref<string | null>(null);

const selectedSpace = computed(() => {
  return (
      spaces.value.find(
          (space) => space.id === selectedSpaceId.value,
      ) ?? null
  );
});

const isLoadingSpaces = ref(false);
const spacesError = ref("");

async function loadSpaces() {
  console.log("loadSpaces called");

  isLoadingSpaces.value = true;
  spacesError.value = "";

  try {
    spaces.value = await tauriApi.listServers();

    selectedSpaceId.value = null;
  } catch (error) {
    console.error("Could not load servers:", error);
    spacesError.value = String(error);
  } finally {
    isLoadingSpaces.value = false;
  }
}
const inviteCopyMessage = ref("");
const inviteCopyError = ref("");

async function copySelectedSpaceInvite() {
  const spaceId = selectedSpaceId.value;

  if (!spaceId) {
    return;
  }

  inviteCopyMessage.value = "";
  inviteCopyError.value = "";

  try {
    const invite =
        await tauriApi.getInvite(spaceId);

    await navigator.clipboard.writeText(invite);

    inviteCopyMessage.value =
        "Invite copied to clipboard.";

    window.setTimeout(() => {
      inviteCopyMessage.value = "";
    }, 2000);
  } catch (error) {
    console.error(
        "Could not copy invite:",
        error,
    );

    inviteCopyError.value = String(error);
  }
}
const rooms = ref<RoomInfo[]>([]);
const selectedRoomId = ref<string | null>(null);

const selectedRoom = computed(() => {
  return (
      rooms.value.find(
          (room) => room.id === selectedRoomId.value,
      ) ?? null
  );
});

const isLoadingRooms = ref(false);
const roomsError = ref("");

const showRoomModal = ref(false);
const roomName = ref("");
const roomError = ref("");
const isCreatingRoom = ref(false);
async function loadRooms(spaceId: string) {
  isLoadingRooms.value = true;
  roomsError.value = "";

  try {
    rooms.value = await tauriApi.listRooms(spaceId);
    selectedRoomId.value =
        rooms.value.length > 0
            ? rooms.value[0].id
            : null;
  } catch (error) {
    console.error("Could not load rooms:", error);
    roomsError.value = String(error);
    rooms.value = [];
    selectedRoomId.value = null;
  } finally {
    isLoadingRooms.value = false;
  }
}
async function submitCreateRoom() {
  if (
      isCreatingRoom.value ||
      !selectedSpaceId.value
  ) {
    return;
  }

  const name = roomName.value.trim();

  if (!name) {
    roomError.value = "Room name cannot be empty.";
    return;
  }

  isCreatingRoom.value = true;
  roomError.value = "";

  try {
    const room = await tauriApi.createRoom(
        selectedSpaceId.value,
        name,
    );

    rooms.value.push(room);
    selectedRoomId.value = room.id;

    closeRoomModal();
  } catch (error) {
    console.error("Could not create room:", error);
    roomError.value = String(error);
  } finally {
    isCreatingRoom.value = false;
  }
}
function openRoom(roomId: string) {
  selectedRoomId.value = roomId;
  messageText.value = "";
  messageError.value = "";
}
async function submitJoinSpace() {
  if (isJoiningSpace.value) {
    return;
  }

  const ticket = serverTicket.value.trim();

  if (!ticket) {
    joinSpaceError.value =
        "Server ticket cannot be empty.";
    return;
  }

  isJoiningSpace.value = true;
  joinSpaceError.value = "";

  try {
    const space = await tauriApi.joinSpace(ticket);

    const alreadyExists = spaces.value.some(
        (existingSpace) =>
            existingSpace.id === space.id,
    );

    if (!alreadyExists) {
      spaces.value.push(space);
    }

    selectedSpaceId.value = space.id;

    await loadRooms(space.id);

    closeServerModal();
  } catch (error) {
    console.error("Could not join space:", error);
    joinSpaceError.value = String(error);
  } finally {
    isJoiningSpace.value = false;
  }
}


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

onMounted( () => {
  void loadChats();
  void loadSpaces();
});

type ServerModalView = "choice" | "join" | "create";
const showServerModal = ref(false);
const serverModalView = ref<ServerModalView>("choice");
const serverTicket = ref("");
const serverName = ref("");

const isCreatingServer = ref(false);
const createServerError = ref("");

const isJoiningSpace = ref(false);
const joinSpaceError = ref("");
function openServerModal() {
  serverModalView.value = "choice";
  serverTicket.value = "";
  serverName.value = "";
  createServerError.value = "";
  joinSpaceError.value="";
  showServerModal.value = true;
}

function closeServerModal() {
  showServerModal.value = false;
}

function showJoinServer() {
  serverTicket.value = "";
  joinSpaceError.value = "";
  serverModalView.value = "join";
}

function showCreateServer() {
  serverName.value ="";
  createServerError.value ="";
  serverModalView.value = "create";
}

function returnToServerChoice() {
  serverModalView.value = "choice";
}

async function submitCreateServer() {
  if(isCreatingServer.value){
    return;
  }
  const name = serverName.value.trim();

  if (!name) {
    createServerError.value = "Server name cannot be empty.";
    return;
  }

  isCreatingServer.value = true;
  createServerError.value = "";

  try {
    const space = await tauriApi.createServer(name);
    spaces.value.push(space);
    selectedSpaceId.value = space.id;
    rooms.value = [];
    selectedRoomId.value = null;

    closeServerModal();
  } catch (error) {
    console.error("Could not create server:", error);
    createServerError.value = String(error);
  } finally {
    isCreatingServer.value = false;
  }
}



function openRoomModal() {
  if (!selectedSpaceId.value) {
    return;
  }

  roomName.value = "";
  roomError.value = "";
  showRoomModal.value = true;
}

function closeRoomModal() {
  showRoomModal.value = false;
}

// TODO: nur zum Testen -> später durch echte Peer-EndpointId aus dem Chat/Space ersetzen
const TEST_PEER_ENDPOINT_ID = "eb6601f820aa05dac332e5ddb7e4a405dba665fbc8e3abf72f1e8bac498a1728";

const isInCall = ref(false);
const callError = ref("");

async function toggleVoiceCall() {
  callError.value = "";

  if (isInCall.value) {
    try {
      await tauriApi.hangUp();
      isInCall.value = false;
    } catch (error) {
      console.error("Could not hang up:", error);
      callError.value = (error as Error).message;
    }
    return;
  }

  try {
    await tauriApi.startCall(TEST_PEER_ENDPOINT_ID);
    isInCall.value = true;
  } catch (error) {
    console.error("Could not start call:", error);
    callError.value = (error as Error).message;
  }
}

function getSpaceInitials(name: string): string {
  const words = name
      .trim()
      .split(/\s+/)
      .filter(Boolean);

  if (words.length === 0) {
    return "?";
  }

  if (words.length === 1) {
    return words[0].slice(0, 2).toUpperCase();
  }

  return words
      .slice(0, 2)
      .map((word) => word.charAt(0))
      .join("")
      .toUpperCase();
}

async function selectSpace(spaceId: string) {
  selectedSpaceId.value = spaceId;
  await loadRooms(spaceId);
}
</script>

<template>
  <div class="app">
    <aside class="server-sidebar">
      <button
          class="server-button home-server"
          title="Blabber home"
          @click ="selectedSpaceId = null"
      >
        B
      </button>

      <div class="server-divider"></div>

      <button
          v-for="space in spaces"
          :key="space.id"
          class="server-button"
          :class="{
      'active-server': selectedSpaceId === space.id,
    }"
          :title="space.name"
          @click="selectSpace(space.id)"
      >
        {{ getSpaceInitials(space.name) }}
      </button>

      <button
          class="server-button add-server"
          title="Create server"
          @click="openServerModal"
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
    <!-- Conversation sidebar -->
    <aside class="chat-sidebar">
      <header class="sidebar-header">
        <div>
          <h1>
            {{ selectedSpace ? selectedSpace.name : "Blabber" }}
          </h1>
          <span>
            {{ selectedSpace ? "Rooms" : "Direct messages" }}
</span>
        </div>

        <div class="sidebar-header-actions">
          <button
              v-if="selectedSpace"
              class="header-action-button"
              type="button"
              title="Copy invite"
              @click="copySelectedSpaceInvite"
          >
            ⧉
          </button>

          <button
              class="new-conversation-button"
              type="button"
              :title="selectedSpace ? 'Create room' : 'Add server'"
              @click="
        selectedSpace
          ? openRoomModal()
          : openServerModal()
      "
          >
            +
          </button>
        </div>
      </header>
      <p
          v-if="inviteCopyMessage"
          class="invite-copy-message"
      >
        {{ inviteCopyMessage }}
      </p>

      <p
          v-if="inviteCopyError"
          class="invite-copy-error"
      >
        {{ inviteCopyError }}
      </p>

      <div
        v-if ="!selectedSpace"
        class= "search-container"
        >
        <input
        v-model="searchQuery"
        type="text"
        placeholder="Search conversations"
        />
      </div>
      <nav class="chat-list">
        <!-- Blabber Home -->
        <template v-if="!selectedSpace">
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
        </template>

        <!-- Ausgewählter Space -->
        <template v-else>
          <p class="backend-status">
            Rooms for {{ selectedSpace.name }}
          </p>
          <p
              v-if="isLoadingRooms"
              class="backend-status"
          >
            Loading rooms...
          </p>
          <p
              v-else-if="roomsError"
              class="backend-status warning"
          >
            {{ roomsError }}
          </p>
          <p
              v-else-if="rooms.length === 0"
              class="no-results"
          >
            No rooms yet.
          </p>

          <button
              v-for="room in rooms"
              :key="room.id"
              class="chat-item"
              :class="{
          selected: selectedRoomId === room.id,
        }"
              @click="openRoom(room.id)"
          >
            <div class="avatar">
              #
            </div>

            <div class="chat-information">
              <strong>{{ room.name }}</strong>
              <span>Text room</span>
            </div>
          </button>

        </template>
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

    <main class="chat-area">
      <!-- Room chat -->
      <template v-if="selectedSpace && selectedRoom">
        <header class="chat-header">
          <div class="chat-user">
            <div class="header-avatar">
              #
            </div>

            <div>
              <h2>{{ selectedRoom.name }}</h2>
              <span>{{ selectedSpace.name }}</span>
            </div>
          </div>

          <div class="chat-actions">
            <button title="Room information">
              ⓘ
            </button>
          </div>
        </header>

        <section class="room-chat-content">
          <div class="room-message-list">

            <div class="room-welcome">
              <h2>Welcome to #{{ selectedRoom.name }}</h2>

              <p>
                This is the beginning of the
                {{ selectedRoom.name }} room.
              </p>
            </div>
          </div>
        </section>

        <footer class="message-bar">
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
              :placeholder="`Message #${selectedRoom.name}`"
          />

          <button
              class="send-button"
              type="button"
              :disabled="!messageText.trim()"
          >
            Send
          </button>
        </footer>
      </template>

      <!-- Space selected, but no room selected -->
      <section
          v-else-if="selectedSpace"
          class="no-chat-selected"
      >
        <div class="empty-icon">
          #
        </div>

        <h2>Select a room</h2>

        <p>
          Choose a room from the list to open it.
        </p>
      </section>

      <!-- Direct message chat -->
      <template v-else-if="selectedChat">
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
            <button
                title="Voice call"
                :class="{ 'call-active': isInCall }"
                @click="toggleVoiceCall"
            >
              {{ isInCall ? "☎ Hang up" : "☎" }}
            </button>

            <button title="Video call">
              ▣
            </button>

            <button title="Conversation information">
              ⓘ
            </button>
          </div>
        </header>

        <p
            v-if="callError"
            class="call-error"
        >
          {{ callError }}
        </p>

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
              :disabled="isJoiningSpace"
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
              :disabled="isJoiningSpace"
              @keydown.enter.prevent="submitJoinSpace"
          />

          <p
              v-if="joinSpaceError"
              class="modal-error"
          >
            {{ joinSpaceError }}
          </p>

          <button
              class="primary-modal-button"
              type="button"
              :disabled="isJoiningSpace"
              @click="submitJoinSpace"
          >
            {{
              isJoiningSpace
                  ? "Joining..."
                  : "Join Server"
            }}
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
              :disabled="isCreatingServer"
              @keyup.enter="submitCreateServer"
          />

          <p v-if="createServerError" class="modal-error">
            {{ createServerError }}
          </p>

          <button
              class="primary-modal-button"
              type="button"
              :disabled="isCreatingServer"
              @click="submitCreateServer"
          >
            {{ isCreatingServer ? "Creating..." : "Create Server" }}
          </button>
        </template>
      </section>
    </div>
  </Teleport>
  <Teleport to="body">
    <div
        v-if="showRoomModal"
        class="modal-backdrop"
        @click.self="closeRoomModal"
    >
      <section class="server-modal">
        <button
            class="modal-close-button"
            type="button"
            @click="closeRoomModal"
        >
          ×
        </button>

        <h2>Create Room</h2>

        <p class="modal-description">
          Create a room in {{ selectedSpace?.name }}.
        </p>

        <label for="room-name">
          Room name
        </label>

        <input
            id="room-name"
            v-model="roomName"
            type="text"
            placeholder="general"
            :disabled="isCreatingRoom"
            @keydown.enter.prevent="submitCreateRoom"
        />

        <p
            v-if="roomError"
            class="modal-error"
        >
          {{ roomError }}
        </p>

        <button
            class="primary-modal-button"
            type="button"
            :disabled="isCreatingRoom"
            @click="submitCreateRoom"
        >
          {{
            isCreatingRoom
                ? "Creating..."
                : "Create Room"
          }}
        </button>
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
  background: #242321;
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
  color: #f7f3e8;
}

/* Server navigation */

.server-sidebar {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 11px;
  padding: 14px 0;
  border-right: 1px solid #45423e;
  background: #242321;
}

.server-button {
  width: 48px;
  height: 48px;
  border-radius: 12px;
  background: #45423e;
  font-weight: 700;
  transition:
      background 150ms ease,
      border-radius 150ms ease,
      transform 150ms ease;
}

.server-button:hover,
.active-server {
  border-radius: 8px;
  background: #f05a24;
  transform: translateY(-1px);
}

.home-server {
  color: #fff7ef;
  background: #f05a24;
}

.server-divider {
  width: 34px;
  height: 1px;
  background: #56514b;
}

.add-server {
  color: #f05a24;
}

.settings-button {
  margin-top: auto;
}

/* Conversation sidebar */

.chat-sidebar {
  display: flex;
  min-width: 0;
  flex-direction: column;
  border-right: 1px solid #45423e;
  background: #302e2b;
}

.sidebar-header {
  display: flex;
  min-height: 76px;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
  border-bottom: 1px solid #45423e;
}
.sidebar-header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.header-action-button {
  width: 34px;
  height: 34px;
  border-radius: 7px;
  color: #c9c3b8;
  background: #45423e;
  font-size: 18px;
  transition:
      background 150ms ease,
      transform 150ms ease;
}

.header-action-button:hover {
  color: #fff7ef;
  background: #f05a24;
  transform: translateY(-1px);
}

.invite-copy-message,
.invite-copy-error {
  margin: 10px 14px 0;
  padding: 9px 11px;
  border-radius: 7px;
  font-size: 12px;
  text-align: center;
}

.invite-copy-message {
  color: #f7f3e8;
  background: #45423e;
}

.invite-copy-error {
  color: #ffb69a;
  background: rgba(240, 90, 36, 0.16);
}

.sidebar-header h1 {
  margin: 0;
  color: #f7f3e8;
  font-size: 20px;
}

.sidebar-header span {
  color: #c9c3b8;
  font-size: 12px;
}

.new-conversation-button {
  width: 34px;
  height: 34px;
  border-radius: 7px;
  color: #f7f3e8;
  background: #45423e;
  font-size: 22px;
  transition:
      background 150ms ease,
      transform 150ms ease;
}

.new-conversation-button:hover {
  background: #f05a24;
  transform: translateY(-1px);
}

.search-container {
  padding: 14px;
}

.search-container input {
  width: 100%;
  height: 40px;
  padding: 0 13px;
  border: 1px solid transparent;
  border-radius: 7px;
  outline: none;
  color: #f7f3e8;
  background: #242321;
  transition: border-color 150ms ease;
}

.search-container input::placeholder {
  color: #8f877d;
}

.search-container input:focus {
  border-color: #f05a24;
}

.chat-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 9px;
}

.backend-status {
  margin: 4px 6px 12px;
  padding: 9px;
  border-radius: 6px;
  color: #c9c3b8;
  background: #45423e;
  font-size: 12px;
  text-align: center;
}

.backend-status.warning {
  color: #ffd3c1;
  background: rgba(240, 90, 36, 0.16);
}

.chat-item {
  display: flex;
  width: 100%;
  align-items: center;
  gap: 12px;
  padding: 10px;
  margin-bottom: 4px;
  border-radius: 7px;
  text-align: left;
  background: transparent;
  transition:
      background 150ms ease,
      transform 150ms ease;
}

.chat-item:hover {
  background: #45423e;
}

.chat-item.selected {
  background: #544f49;
  box-shadow: inset 4px 0 0 #f05a24;
}

.avatar,
.header-avatar,
.large-avatar,
.profile-avatar,
.empty-icon {
  display: grid;
  flex-shrink: 0;
  place-items: center;
  border-radius: 10px;
  color: #fff7ef;
  font-weight: 700;
  background: #f05a24;
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
  border: 3px solid #302e2b;
  border-radius: 50%;
  background: #8b857d;
}

.status.online {
  background: #f05a24;
}

.chat-information {
  display: flex;
  min-width: 0;
  flex-direction: column;
}

.chat-information strong {
  overflow: hidden;
  color: #f7f3e8;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-information span {
  margin-top: 3px;
  color: #c9c3b8;
  font-size: 12px;
}

.no-results {
  padding: 20px;
  color: #c9c3b8;
  text-align: center;
}

.profile {
  display: flex;
  min-height: 64px;
  align-items: center;
  gap: 10px;
  padding: 10px;
  background: #242321;
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
  color: #f7f3e8;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.profile-information span {
  color: #f05a24;
  font-size: 12px;
}

.logout-button {
  width: 34px;
  height: 34px;
  padding: 7px;
  border-radius: 6px;
  color: #c9c3b8;
  background: transparent;
  font-size: 18px;
}

.logout-button:hover:not(:disabled) {
  color: #fff;
  background: #45423e;
}

.logout-button:disabled {
  opacity: 0.5;
}

/* Main chat area */

.chat-area {
  display: flex;
  min-width: 0;
  flex-direction: column;
  background: #383633;
}

.chat-header {
  display: flex;
  min-height: 76px;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
  border-bottom: 1px solid #45423e;
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
  color: #f7f3e8;
  font-size: 17px;
}

.chat-user span {
  color: #c9c3b8;
  font-size: 12px;
}

.chat-actions {
  display: flex;
  gap: 8px;
}

.chat-actions button {
  width: 38px;
  height: 38px;
  border-radius: 7px;
  color: #c9c3b8;
  background: transparent;
}

.chat-actions button:hover {
  color: #fff;
  background: #45423e;
}

.chat-actions button.call-active {
  color: #fff7ef;
  background: #f05a24;
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
  color: #f7f3e8;
}

.empty-chat p,
.no-chat-selected p {
  margin: 0;
  color: #c9c3b8;
}

.empty-icon {
  width: 90px;
  height: 90px;
  font-size: 34px;
}

.message-error {
  margin: 0 22px 10px;
  color: #ffb69a;
  font-size: 13px;
}

.call-error {
  margin: 0 22px 10px;
  color: #ffb69a;
  font-size: 13px;
}

.message-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0 22px 22px;
  padding: 9px 10px;
  border: 1px solid #56514b;
  border-radius: 8px;
  background: #45423e;
}

.message-bar input {
  min-width: 0;
  height: 40px;
  flex: 1;
  border: none;
  outline: none;
  color: #f7f3e8;
  background: transparent;
}

.message-bar input::placeholder {
  color: #a79f95;
}

.message-bar button {
  padding: 9px 14px;
  border-radius: 6px;
  background: #5a5651;
}

.attachment-button {
  color: #c9c3b8;
}

.send-button {
  color: #fff7ef;
  background: #f05a24 !important;
}

.send-button:hover:not(:disabled) {
  background: #d94c1b !important;
}

.send-button:disabled {
  opacity: 0.5;
}
/* Room chat */

.room-chat-content {
  min-height: 0;
  flex: 1;
  overflow: hidden;
}

.room-message-list {
  display: flex;
  height: 100%;
  min-height: 0;
  flex-direction: column;
  overflow-y: auto;
  padding: 24px 28px;
}

.room-welcome {
  margin-top: auto;
  padding-bottom: 8px;
}

.room-welcome h2 {
  margin: 0 0 8px;
  color: #f7f3e8;
  font-size: 28px;
}

.room-welcome p {
  margin: 0;
  color: #c9c3b8;
  font-size: 15px;
}

.room-message-bar {
  flex-shrink: 0;
}

.modal-backdrop {
  position: fixed;
  z-index: 1000;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 24px;
  background: rgba(20, 19, 18, 0.8);
  backdrop-filter: blur(4px);
}

.server-modal {
  position: relative;
  width: min(420px, 100%);
  padding: 32px;
  border: 1px solid #5a5651;
  border-radius: 10px;
  color: #f7f3e8;
  background: #302e2b;
  box-shadow: 0 30px 90px rgba(0, 0, 0, 0.55);
}

.server-modal h2 {
  margin: 0;
  color: #f7f3e8;
  text-align: center;
}

.modal-description {
  margin: 10px 0 24px;
  color: #c9c3b8;
  text-align: center;
}

.modal-error {
  margin-top: 8px;
  color: #ffb69a;
  font-size: 13px;
  text-align: center;
}

.modal-close-button {
  position: absolute;
  top: 12px;
  right: 16px;
  border: none;
  color: #c9c3b8;
  background: transparent;
  font-size: 28px;
  cursor: pointer;
}

.modal-close-button:hover {
  color: #f05a24;
}

.server-choice-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.server-choice-button {
  height: 48px;
  border: 1px solid #6a635b;
  border-radius: 7px;
  color: #f7f3e8;
  background: #45423e;
  font-weight: 700;
  cursor: pointer;
  transition:
      border-color 150ms ease,
      background 150ms ease,
      transform 150ms ease;
}

.server-choice-button:hover {
  border-color: #f05a24;
  background: #514d48;
  transform: translateY(-1px);
}

.server-choice-button:first-child {
  border-color: #f05a24;
  background: #f05a24;
}

.server-choice-button:first-child:hover {
  background: #d94c1b;
}

.modal-back-button {
  margin-bottom: 18px;
  padding: 0;
  border: none;
  color: #c9c3b8;
  background: transparent;
  cursor: pointer;
}

.modal-back-button:hover {
  color: #f05a24;
}

.server-modal label {
  display: block;
  margin-bottom: 8px;
  color: #c9c3b8;
  font-size: 13px;
  font-weight: 600;
}

.server-modal input {
  width: 100%;
  height: 48px;
  padding: 0 14px;
  border: 1px solid #56514b;
  border-radius: 7px;
  outline: none;
  color: #f7f3e8;
  background: #242321;
  transition:
      border-color 150ms ease,
      box-shadow 150ms ease;
}

.server-modal input::placeholder {
  color: #8f877d;
}

.server-modal input:focus {
  border-color: #f05a24;
  box-shadow: 0 0 0 3px rgba(240, 90, 36, 0.12);
}

.primary-modal-button {
  width: 100%;
  height: 46px;
  margin-top: 18px;
  border: none;
  border-radius: 7px;
  color: #fff7ef;
  background: #f05a24;
  font-weight: 700;
  cursor: pointer;
  transition:
      background 150ms ease,
      transform 150ms ease;
}

.primary-modal-button:hover {
  background: #d94c1b;
  transform: translateY(-1px);
}

.primary-modal-button:disabled,
.server-choice-button:disabled,
.modal-close-button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

/* Scrollbars */

.chat-list::-webkit-scrollbar {
  width: 8px;
}

.chat-list::-webkit-scrollbar-track {
  background: transparent;
}

.chat-list::-webkit-scrollbar-thumb {
  border: 2px solid transparent;
  border-radius: 999px;
  background: #5a5651;
  background-clip: padding-box;
}

.chat-list::-webkit-scrollbar-thumb:hover {
  background: #6a635b;
  background-clip: padding-box;
}

/* Smaller windows */

@media (max-width: 850px) {
  .app {
    grid-template-columns:
        64px 250px minmax(0, 1fr);
  }

  .server-button {
    width: 44px;
    height: 44px;
  }

  .sidebar-header {
    padding: 0 14px;
  }

  .chat-header {
    padding: 0 18px;
  }
}

@media (max-width: 650px) {
  .app {
    grid-template-columns:
        58px 210px minmax(0, 1fr);
  }

  .server-sidebar {
    gap: 8px;
  }

  .server-button {
    width: 40px;
    height: 40px;
  }

  .chat-sidebar {
    font-size: 14px;
  }

  .sidebar-header h1 {
    font-size: 17px;
  }

  .chat-header {
    padding: 0 14px;
  }

  .message-bar {
    margin-right: 14px;
    margin-left: 14px;
  }
}
</style>