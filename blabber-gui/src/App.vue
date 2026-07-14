<script setup lang="ts">
import { ref } from "vue";
import LoginView from "./views/LoginView.vue";
import ChatView from "./views/ChatView.vue";
import SettingsView from "./views/SettingsView.vue";
import type { User } from "./API/tauriAPI";

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

</template>