<script setup lang="ts">
import { onMounted,ref } from "vue";
import { tauriApi } from "../API/tauriAPI";
import type { User } from "../API/tauriAPI";

const emit = defineEmits<{
  (event: "login", user: User): void;
}>();

const username = ref("");
const password = ref("");

const identities = ref<string[]>([]);
const identitiesOpen = ref(false);
const identitiesError = ref("");
const isLoadingIdentities = ref(false)

async function loadIdentities() {
  isLoadingIdentities.value = true;
  identitiesError.value = "";
  try {
    identities.value = await tauriApi.listIdentities();
  } catch (error) {
    console.error("Could not load identities:", error);
    identitiesError.value = String(error);
  } finally {
    isLoadingIdentities.value = false;
  }

}

function toggleIdentities() {
  identitiesOpen.value = !identitiesOpen.value;
}

function selectIdentity(identityName: string) {
  username.value = identityName;
  identitiesOpen.value = false;
  loginError.value = "";
}

const isLoading = ref(false);
const isRegistering = ref(false);
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
    loginError.value = String(error);
  } finally {
    isLoading.value = false;
  }
}

async function submitRegister() {
  const cleanUsername = username.value.trim();
  if (!cleanUsername || !password.value) {
    loginError.value =
        "Please enter your username and password.";
    return;
  }
  isRegistering.value = true;
  loginError.value = "";
  try {
    const user = await tauriApi.createIdentity(
        cleanUsername,
        password.value,
    );
    emit("login", user);
  } catch (error) {
    console.error("Identity creation failed:", error);
    loginError.value = String(error);
  } finally {
    isRegistering.value = false;
  }
}
onMounted(() => {
  loadIdentities();
});
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
            :disabled="isLoading || isRegistering"
        >
          {{ isLoading ? "Logging in..." : "Log in" }}
        </button>

        <button
            class="register-button"
            type="button"
            :disabled="isLoading || isRegistering"
            @click="submitRegister"
        >
          {{
            isRegistering
                ? "Creating New Account..."
                : "Create Account"
          }}
        </button>
        <div class="identity-accordion">
          <button
              class="identity-accordion-button"
              type="button"
              :aria-expanded="identitiesOpen"
              @click="toggleIdentities"
          >
    <span>
      Existing identities
    </span>
            <span
                class="identity-arrow"
                :class="{ open: identitiesOpen }"
            >
      ▼
    </span>
          </button>
          <div
              v-if="identitiesOpen"
              class="identity-accordion-content"
          >
            <p
                v-if="isLoadingIdentities"
                class="identity-status"
            >
              Loading identities...
            </p>
            <p
                v-else-if="identitiesError"
                class="identity-status identity-error"
            >
              {{ identitiesError }}
            </p>
            <p
                v-else-if="identities.length === 0"
                class="identity-status"
            >
              No identities found.
            </p>
            <button
                v-for="identity in identities"
                v-else
                :key="identity"
                class="identity-item"
                type="button"
                @click="selectIdentity(identity)"
            >
      <span class="identity-avatar">
        {{ identity.charAt(0).toUpperCase() }}
      </span>

              <span class="identity-name">
        {{ identity }}
      </span>
            </button>
          </div>
        </div>
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
:global(body),
:global(#app) {
  overflow-y: auto;
}

:global(body) {
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

.login-page {
  display: grid;
  width: 100%;
  min-height: 100%;
  place-items: start center;
  padding: 40px 24px;
  overflow-y: auto;
  color: #f7f3e8;
  background:
      radial-gradient(
          circle at top left,
          rgba(240, 90, 36, 0.12),
          transparent 35%
      ),
      radial-gradient(
          circle at bottom right,
          rgba(201, 195, 184, 0.08),
          transparent 40%
      ),
      #242321;
}

.login-card {
  width: min(420px, 100%);
  margin: auto 0;
  padding: 38px;
  border: 1px solid #45423e;
  border-radius: 12px;
  background: #302e2b;
  box-shadow: 0 25px 70px rgba(0, 0, 0, 0.45);
}

.logo {
  display: grid;
  width: 64px;
  height: 64px;
  margin: 0 auto 22px;
  place-items: center;
  border-radius: 12px;
  font-size: 27px;
  font-weight: 800;
  color: #f7f3e8;
  background: #f05a24;
}

.login-heading {
  margin-bottom: 28px;
  text-align: center;
}

.login-heading h1 {
  margin: 0;
  font-size: 26px;
  color: #f7f3e8;
}

.login-heading p {
  margin: 9px 0 0;
  color: #c9c3b8;
  font-size: 14px;
}

form {
  display: flex;
  flex-direction: column;
}

label {
  margin: 0 0 7px;
  color: #c9c3b8;
  font-size: 13px;
  font-weight: 600;
}

input {
  width: 100%;
  height: 46px;
  margin-bottom: 18px;
  padding: 0 14px;
  border: 1px solid transparent;
  border-radius: 8px;
  outline: none;
  color: #f7f3e8;
  background: #242321;
  transition: border-color 150ms ease;
}

input::placeholder {
  color: #8f877d;
}

input:focus {
  border-color: #f05a24;
}

input:disabled {
  opacity: 0.65;
}

.login-error {
  margin: -5px 0 16px;
  padding: 10px 12px;
  border-radius: 8px;
  color: #f7f3e8;
  background: rgba(240, 90, 36, 0.15);
  font-size: 13px;
}

.login-button,
.register-button {
  height: 46px;
  border: none;
  border-radius: 8px;
  color: #f7f3e8;
  background: #f05a24;
  font-weight: 700;
  cursor: pointer;
  transition:
      background 150ms ease,
      transform 120ms ease;
}

.register-button {
  margin-top: 12px;
}

.login-button:hover:not(:disabled),
.register-button:hover:not(:disabled) {
  background: #d94c1b;
  transform: translateY(-1px);
}

.login-button:disabled,
.register-button:disabled {
  opacity: 0.65;
  cursor: wait;
}

.login-footer {
  margin: 24px 0 0;
  color: #a79f95;
  font-size: 12px;
  text-align: center;
}
.identity-accordion {
  margin-top: 16px;
  margin-bottom: 22px;
  border: 1px solid #45423e;
  border-radius: 8px;
  overflow: hidden;
  background: #242321;
}

.identity-accordion-button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  min-height: 46px;
  padding: 0 14px;
  border: none;
  color: #f7f3e8;
  background: transparent;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}

.identity-accordion-button:hover {
  background: #383633;
}

.identity-arrow {
  color: #f05a24;
  font-size: 11px;
  transition: transform 150ms ease;
}

.identity-arrow.open {
  transform: rotate(180deg);
}

.identity-accordion-content {
  padding: 6px;
  border-top: 1px solid #45423e;
}

.identity-item {
  display: flex;
  align-items: center;
  width: 100%;
  min-height: 46px;
  padding: 7px 9px;
  border: none;
  border-radius: 7px;
  color: #f7f3e8;
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.identity-item:hover {
  background: #383633;
}

.identity-avatar {
  display: grid;
  width: 32px;
  height: 32px;
  margin-right: 11px;
  place-items: center;
  flex-shrink: 0;
  border-radius: 8px;
  color: #f7f3e8;
  background: #f05a24;
  font-size: 13px;
  font-weight: 800;
}

.identity-name {
  overflow: hidden;
  font-size: 14px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.identity-status {
  margin: 0;
  padding: 12px;
  color: #a79f95;
  font-size: 13px;
  text-align: center;
}

.identity-error {
  color: #f05a24;
}
</style>