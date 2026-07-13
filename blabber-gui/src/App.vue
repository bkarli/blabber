<script setup lang="ts">
import { ref } from "vue";
import LoginView from "./views/LoginView.vue";
import ChatView from "./views/ChatView.vue";
import type { User } from "./API/tauriAPI";

// true = Login überspringen
// false = normale Login-Seite anzeigen
const skipLogin = ref(true);

const testUser: User = {
  id: "test-user",
  username: "Test User",
  initials: "TU",
};

const currentUser = ref<User | null>(null);

function handleLogin(user: User) {
  currentUser.value = user;
}

function handleLogout() {
  currentUser.value = null;
}
</script>

<template>
  <LoginView
      v-if="!skipLogin && currentUser === null"
      @login="handleLogin"
  />

  <ChatView
      v-else
      :user="currentUser ?? testUser"
      @logout="handleLogout"
  />
</template>