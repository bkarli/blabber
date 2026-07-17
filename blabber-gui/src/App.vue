<script setup lang="ts">
import LoginView from "./views/LoginView.vue";
import ChatView from "./views/ChatView.vue";
import SettingsView from "./views/SettingsView.vue";
import type { User } from "./API/tauriAPI";
import { onMounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { tauriApi } from "./API/tauriAPI";

// true = Login überspringen
// false = normale Login-Seite anzeigen
type AppView = "login"|"chat"|"settings";

const skipLogin = ref(false);

const testUser: User = {
  displayName: "Test User",
};
const currentView = ref<AppView>(
    skipLogin.value ? "chat":"login",
);

const currentUser = ref<User | null>(
    skipLogin.value ? testUser : null,
);

function handleLogin(user: User) {
  currentUser.value = user;
  currentView.value = "chat";
}

function handleLogout() {
  currentUser.value = null;
  currentView.value = "login";
}

function openSettings(){
  currentView.value = "settings";
}
function closeSettings(){
  currentView.value = "chat";
}

const incomingCallPeerId = ref<string | null>(null);

onMounted(() => {
  listen<string>("incoming_call", (event) => {
    incomingCallPeerId.value = event.payload;
  });
});

async function acceptCall() {
  await tauriApi.answerCall(true);
  incomingCallPeerId.value = null;
}

async function declineCall() {
  await tauriApi.answerCall(false);
  incomingCallPeerId.value = null;
}


</script>

<template>

  <LoginView
      v-if="currentView === 'login'"
      @login="handleLogin"

  />
  <ChatView
      v-else-if="currentView === 'chat'"
      v-bind:user="currentUser ?? testUser"
      @logout="handleLogout"
      @open-settings="openSettings"
  />

  <SettingsView
      v-else-if="currentView === 'settings'"
      @back="closeSettings"
      @logout="handleLogout"

  />
  <Teleport to="body">
    <div v-if="incomingCallPeerId" class="incoming-call-backdrop">
      <div class="incoming-call-card">
        <p>Eingehender Anruf</p>
        <p class="peer-id">{{ incomingCallPeerId }}</p>
        <div class="incoming-call-actions">
          <button class="decline-button" @click="declineCall">Ablehnen</button>
          <button class="accept-button" @click="acceptCall">Annehmen</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.incoming-call-backdrop {
  position: fixed;
  z-index: 2000;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(20, 19, 18, 0.8);
}
.incoming-call-card {
  padding: 24px;
  border-radius: 10px;
  background: #302e2b;
  color: #f7f3e8;
  text-align: center;
}
.peer-id {
  font-size: 12px;
  color: #c9c3b8;
  word-break: break-all;
  margin: 8px 0 20px;
}
.incoming-call-actions {
  display: flex;
  gap: 12px;
}
.accept-button {
  background: #f05a24;
  color: #fff7ef;
  padding: 10px 18px;
  border-radius: 7px;
}
.decline-button {
  background: #45423e;
  color: #f7f3e8;
  padding: 10px 18px;
  border-radius: 7px;
}
</style>

