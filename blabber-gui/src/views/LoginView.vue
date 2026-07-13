<script setup lang="ts">
import { ref } from "vue";

const emit = defineEmits<{
  (event: "login", username: string): void;
}>();

const username = ref("");
const password = ref("");

const showPassword = ref(false);
const rememberMe = ref(false);
const loginError = ref("");
const isLoading = ref(false);

async function handleLogin() {
  loginError.value = "";

  if (!username.value.trim()) {
    loginError.value = "Please enter your username.";
    return;
  }

  if (!password.value) {
    loginError.value = "Please enter your password.";
    return;
  }

  isLoading.value = true;

  try {
    /*
     * Temporärer Frontend-Login.
     *
     * Später wird hier euer Tauri-Backend aufgerufen:
     *
     * await invoke("login", {
     *   username: username.value,
     *   password: password.value,
     * });
     */

    await new Promise((resolve) => {
      setTimeout(resolve, 400);
    });

    emit("login", username.value.trim());
  } catch (error) {
    console.error("Login failed:", error);
    loginError.value = "Login failed. Please try again.";
  } finally {
    isLoading.value = false;
  }
}
</script>

<template>
  <main class="login-page">
    <div class="background-decoration">
      <div class="background-circle circle-one"></div>
      <div class="background-circle circle-two"></div>
      <div class="background-circle circle-three"></div>
    </div>

    <section class="login-card">
      <div class="logo">
        B
      </div>

      <header class="login-header">
        <h1>Welcome back</h1>

        <p>
          Log in to continue to Blabber.
        </p>
      </header>

      <form
          class="login-form"
          @submit.prevent="handleLogin"
      >
        <label class="form-field">
          <span>Username</span>

          <input
              v-model="username"
              type="text"
              placeholder="Enter your username"
              autocomplete="username"
              autofocus
          />
        </label>

        <label class="form-field">
          <span>Password</span>

          <div class="password-container">
            <input
                v-model="password"
                :type="showPassword ? 'text' : 'password'"
                placeholder="Enter your password"
                autocomplete="current-password"
            />

            <button
                type="button"
                class="show-password-button"
                @click="showPassword = !showPassword"
            >
              {{ showPassword ? "Hide" : "Show" }}
            </button>
          </div>
        </label>

        <div class="login-options">
          <label class="remember-option">
            <input
                v-model="rememberMe"
                type="checkbox"
            />

            <span>Remember me</span>
          </label>

          <button
              type="button"
              class="text-button"
          >
            Forgot password?
          </button>
        </div>

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

      <footer class="register-text">
        <span>New to Blabber?</span>

        <button type="button">
          Create an account
        </button>
      </footer>
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

button {
  border: none;
}

.login-page {
  position: relative;
  display: flex;
  width: 100vw;
  min-height: 100vh;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  padding: 30px;
  color: #f2f3f5;
  background:
      radial-gradient(
          circle at 15% 15%,
          rgba(88, 101, 242, 0.34),
          transparent 35%
      ),
      radial-gradient(
          circle at 85% 85%,
          rgba(139, 92, 246, 0.28),
          transparent 38%
      ),
      #111318;
}

.background-decoration {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
}

.background-circle {
  position: absolute;
  border-radius: 50%;
  filter: blur(3px);
}

.circle-one {
  top: -180px;
  left: -160px;
  width: 500px;
  height: 500px;
  background: rgba(88, 101, 242, 0.14);
}

.circle-two {
  right: -220px;
  bottom: -220px;
  width: 600px;
  height: 600px;
  background: rgba(139, 92, 246, 0.13);
}

.circle-three {
  top: 18%;
  right: 18%;
  width: 160px;
  height: 160px;
  background: rgba(88, 101, 242, 0.08);
}

.login-card {
  position: relative;
  z-index: 1;
  width: min(430px, 100%);
  padding: 38px;
  border: 1px solid #30333b;
  border-radius: 18px;
  background: rgba(29, 31, 38, 0.96);
  box-shadow: 0 30px 70px rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(18px);
}

.logo {
  display: grid;
  width: 66px;
  height: 66px;
  place-items: center;
  margin: 0 auto 22px;
  border-radius: 20px;
  color: white;
  background: linear-gradient(145deg, #5865f2, #8b5cf6);
  box-shadow: 0 12px 30px rgba(88, 101, 242, 0.28);
  font-size: 30px;
  font-weight: 800;
}

.login-header {
  margin-bottom: 28px;
  text-align: center;
}

.login-header h1 {
  margin: 0 0 8px;
  font-size: 27px;
}

.login-header p {
  margin: 0;
  color: #9499a6;
  font-size: 14px;
}

.login-form {
  display: flex;
  flex-direction: column;
  gap: 19px;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.form-field > span {
  color: #c5c7ce;
  font-size: 13px;
  font-weight: 600;
}

.form-field input {
  width: 100%;
  height: 48px;
  padding: 0 14px;
  border: 1px solid #343740;
  border-radius: 9px;
  outline: none;
  color: white;
  background: #121419;
  transition:
      border-color 150ms ease,
      box-shadow 150ms ease;
}

.form-field input::placeholder {
  color: #747982;
}

.form-field input:focus {
  border-color: #5865f2;
  box-shadow: 0 0 0 3px rgba(88, 101, 242, 0.16);
}

.password-container {
  position: relative;
}

.password-container input {
  padding-right: 68px;
}

.show-password-button {
  position: absolute;
  top: 50%;
  right: 11px;
  padding: 5px;
  transform: translateY(-50%);
  color: #8c96ff;
  background: transparent;
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
}

.show-password-button:hover {
  color: #adb4ff;
}

.login-options {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: -3px;
}

.remember-option {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #9499a6;
  cursor: pointer;
  font-size: 13px;
}

.remember-option input {
  accent-color: #5865f2;
}

.text-button {
  padding: 0;
  color: #8c96ff;
  background: transparent;
  cursor: pointer;
  font-size: 13px;
}

.text-button:hover {
  text-decoration: underline;
}

.login-error {
  margin: -4px 0 0;
  padding: 10px 12px;
  border: 1px solid rgba(237, 66, 69, 0.3);
  border-radius: 8px;
  color: #f0b8b8;
  background: rgba(237, 66, 69, 0.1);
  font-size: 13px;
}

.login-button {
  height: 48px;
  border-radius: 9px;
  color: white;
  background: #5865f2;
  cursor: pointer;
  font-weight: 700;
  transition:
      background 150ms ease,
      transform 150ms ease;
}

.login-button:hover:not(:disabled) {
  background: #6874f5;
  transform: translateY(-1px);
}

.login-button:disabled {
  opacity: 0.65;
  cursor: not-allowed;
}

.register-text {
  display: flex;
  justify-content: center;
  gap: 6px;
  margin-top: 24px;
  color: #9499a6;
  font-size: 13px;
}

.register-text button {
  padding: 0;
  color: #8c96ff;
  background: transparent;
  cursor: pointer;
}

.register-text button:hover {
  text-decoration: underline;
}

@media (max-width: 520px) {
  .login-page {
    padding: 18px;
  }

  .login-card {
    padding: 30px 22px;
  }

  .login-options {
    align-items: flex-start;
    flex-direction: column;
    gap: 12px;
  }
}
</style>