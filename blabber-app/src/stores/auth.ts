import {defineStore} from 'pinia';
import {api} from '@/lib/tauri';
import {useAppStore} from './app';
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
      this.displayName = await api.createIdentity(displayName, password);
      await useAppStore().init();
      router.push({ name: 'spaces' });
    },
    async login(displayName: string, password: string) {
      this.displayName = await api.login(displayName, password);
      await useAppStore().init();
      router.push({ name: 'spaces' });
    },
    async logout() {
      await api.logout();
      // the backend hung up any active call; the call overlay lives outside the router
      useAppStore().resetCallState();
      this.displayName = null;
      router.push({ name: 'login' });
    },
  },
});
