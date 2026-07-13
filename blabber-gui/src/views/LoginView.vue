<script setup lang="ts">
import { ref } from "vue";
import { tauriApi } from "../API/tauriAPI";
import type { User } from "../API/tauriAPI";

const emit = defineEmits<{
  (event: "login", user: User): void;
}>();

const username = ref("");
const password = ref("");

const isLoading = ref(false);
const loginError = ref("");

async function submitLogin() {
  const cleanUsername = username.value.trim();

  if (!cleanUsername || !password.value) {
    loginError.value =
        "Please enter your username and password.";

    return;
  }

  isLoading.value = true;
  loginError.value = "";

  try {
    const user = await tauriApi.login(
        cleanUsername,
        password.value,
    );

    emit("login", user);
  } catch (error) {
    console.error("Login failed:", error);

    loginError.value =
        "Login failed. Please check your information.";
  } finally {
    isLoading.value = false;
  }
}
</script>

<template>
  <main class="login-page">
    <section class="login-card">
      <div class="logo">
        B
      </div>

      <div class="login-heading">
        <h1>Welcome to Blabber</h1>

        <p>
          Log in to continue to your conversations.
        </p>
      </div>

      <form @submit.prevent="submitLogin">
        <label for="username">
          Username
        </label>

        <input
            id="username"
            v-model="username"
            type="text"
            autocomplete="username"
            placeholder="Enter your username"
            :disabled="isLoading"
        />

        <label for="password">
          Password
        </label>

        <input
            id="password"
            v-model="password"
            type="password"
            autocomplete="current-password"
            placeholder="Enter your password"
            :disabled="isLoading"
        />

        <p
            v-if="loginError"
            class="login-error"
        >
          {{ loginError }}
        </p>

        <button
            class="login-button"
            type="submit"
            :disabled="isLoading"
        >
          {{ isLoading ? "Logging in..." : "Log in" }}
        </button>
      </form>

      <p class="login-footer">
        Peer-to-peer communication with Blabber.
      </p>
    </section>
  </main>
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

.login-page {
  display: grid;
  width: 100vw;
  min-height: 100vh;
  place-items: center;
  padding: 24px;
  color: #f2f3f5;
  background:
      radial-gradient(
          circle at top left,
          rgba(88, 101, 242, 0.25),
          transparent 36%
      ),
      radial-gradient(
          circle at bottom right,
          rgba(139, 92, 246, 0.18),
          transparent 34%
      ),
      #111318;
}

.login-card {
  width: min(420px, 100%);
  padding: 38px;
  border: 1px solid #30333b;
  border-radius: 20px;
  background: #1d1f26;
  box-shadow: 0 25px 70px rgba(0, 0, 0, 0.35);
}

.logo {
  display: grid;
  width: 64px;
  height: 64px;
  margin: 0 auto 22px;
  place-items: center;
  border-radius: 20px;
  font-size: 27px;
  font-weight: 800;
  background: linear-gradient(145deg, #5865f2, #8b5cf6);
}

.login-heading {
  margin-bottom: 28px;
  text-align: center;
}

.login-heading h1 {
  margin: 0;
  font-size: 26px;
}

.login-heading p {
  margin: 9px 0 0;
  color: #9499a6;
  font-size: 14px;
}

form {
  display: flex;
  flex-direction: column;
}

label {
  margin: 0 0 7px;
  color: #b5bac1;
  font-size: 13px;
  font-weight: 600;
}

input {
  width: 100%;
  height: 46px;
  margin-bottom: 18px;
  padding: 0 14px;
  border: 1px solid transparent;
  border-radius: 10px;
  outline: none;
  color: white;
  background: #121419;
}

input:focus {
  border-color: #5865f2;
}

input:disabled {
  opacity: 0.65;
}

.login-error {
  margin: -5px 0 16px;
  padding: 10px 12px;
  border-radius: 8px;
  color: #f0b8b8;
  background: rgba(237, 66, 69, 0.12);
  font-size: 13px;
}

.login-button {
  height: 46px;
  border: none;
  border-radius: 10px;
  color: white;
  background: #5865f2;
  font-weight: 700;
  cursor: pointer;
}

.login-button:hover:not(:disabled) {
  background: #4752c4;
}

.login-button:disabled {
  opacity: 0.65;
  cursor: wait;
}

.login-footer {
  margin: 24px 0 0;
  color: #747982;
  font-size: 12px;
  text-align: center;
}
</style>