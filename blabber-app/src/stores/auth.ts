import { defineStore } from 'pinia';
import { api } from '@/lib/tauri';
import { useAppStore } from './app';
import router from '@/router';

export const useAuthStore = defineStore('auth', {
  state: () => ({
    displayName: null as string | null,
  }),
  getters: {
    isLoggedIn: (state) => state.displayName !== null,
  },
  actions: {
    async createIdentity(displayName: string, password: string) {
      const name = await api.createIdentity(displayName, password);
      this.displayName = name;
      await useAppStore().init();
      router.push({ name: 'spaces' });
    },
    async login(displayName: string, password: string) {
      const name = await api.login(displayName, password);
      this.displayName = name;
      await useAppStore().init();
      router.push({ name: 'spaces' });
    },
    async logout() {
      await api.logout();
      this.displayName = null;
      router.push({ name: 'login' });
    },
  },
});
