<script setup lang="ts">
import { ref } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { Button } from '@/components/ui/button';
import { Settings } from 'lucide-vue-next';
import IdentitySettingsDrawer from '@/components/settings/IdentitySettingsDrawer.vue';

const auth = useAuthStore();
const settingsOpen = ref(false);
</script>

<template>
  <div class="flex h-16 shrink-0 items-center gap-3 border-t border-border px-3">
    <div class="flex h-10 w-10 items-center justify-center rounded-full bg-primary text-sm font-semibold text-primary-foreground">
      Me
    </div>
    <div class="flex-1 min-w-0">
      <p class="truncate text-sm font-medium">{{ auth.displayName ?? 'You' }}</p>
      <p class="truncate text-xs text-muted-foreground">Online</p>
    </div>
    <Button variant="ghost" size="icon" class="h-8 w-8 text-muted-foreground hover:text-foreground" @click="settingsOpen = true">
      <Settings class="h-4 w-4" />
    </Button>
  </div>

  <IdentitySettingsDrawer v-model:open="settingsOpen" />
</template>
