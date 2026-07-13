<script setup lang="ts">
const emit = defineEmits<{
  (event: "back"): void;
  (event: "logout"): void;
}>();

function goBack() {
  emit("back");
}

function logout() {
  emit("logout");
}
</script>

<template>
  <div class="settings-container">
    <div class="settings-header">
      <button class="back-button" @click="goBack">
        ← Back
      </button>

      <h1>Settings</h1>
    </div>

    <div class="settings-section">
      <h2>Appearance</h2>

      <label class="setting-item">
        <span>Dark Mode</span>
        <input type="checkbox" checked />
      </label>
    </div>

    <div class="settings-section">
      <h2>Notifications</h2>

      <label class="setting-item">
        <span>Enable Notifications</span>
        <input type="checkbox" checked />
      </label>
    </div>

    <div class="settings-section">
      <h2>Account</h2>

      <button class="logout-button" @click="logout">
        Log Out
      </button>
    </div>
  </div>
</template>

<style scoped>
.settings-container {
  max-width: 700px;
  margin: 40px auto;
  padding: 24px;
  color: white;
  background: #313338;
  border-radius: 12px;
  font-family: Arial, sans-serif;
}

.settings-header {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 30px;
}

.back-button {
  background: #4f545c;
  color: white;
  border: none;
  padding: 8px 16px;
  border-radius: 8px;
  cursor: pointer;
}

.back-button:hover {
  background: #5d6269;
}

.settings-section {
  margin-bottom: 30px;
}

.settings-section h2 {
  margin-bottom: 15px;
  color: #b5bac1;
}

.setting-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 0;
}

.logout-button {
  background: #da373c;
  color: white;
  border: none;
  padding: 10px 18px;
  border-radius: 8px;
  cursor: pointer;
  font-size: 15px;
}

.logout-button:hover {
  background: #c12d32;
}
</style>