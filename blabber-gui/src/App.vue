<script setup lang="ts">
import { ref } from "vue";
import LoginView from "./views/LoginView.vue";
import ChatView from "./views/ChatView.vue";

const isLoggedIn = ref(true);
const username = ref("");

function handleLogin(name: string) {
  username.value = name;
  isLoggedIn.value = true;
}

function handleLogout() {
  username.value = "";
  isLoggedIn.value = false;
}
</script>

<template>
  <LoginView
      v-if="!isLoggedIn"
      @login="handleLogin"
  />

  <ChatView
      v-else
      :username="username"
      @logout="handleLogout"
  />
</template>