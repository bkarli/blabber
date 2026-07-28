<script setup lang="ts">
import { ref } from 'vue';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useAppStore } from '@/stores/app';

const open = defineModel<boolean>('open', { default: false });
const props = defineProps<{ spaceId: string }>();
const store = useAppStore();

const channelName = ref('');
const creating = ref(false);
const error = ref<string | null>(null);

async function handleCreate() {
  error.value = null;
  if (!channelName.value.trim()) {
    error.value = 'Channel name is required.';
    return;
  }
  creating.value = true;
  try {
    await store.createChannel(props.spaceId, channelName.value.trim());
    channelName.value = '';
    open.value = false;
  } catch (e) {
    error.value = String(e);
  } finally {
    creating.value = false;
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-sm">
      <DialogHeader>
        <DialogTitle>Create a Channel</DialogTitle>
      </DialogHeader>
      <div class="space-y-4 pt-2">
        <div class="space-y-2">
          <Label for="channel-name">Channel Name</Label>
          <Input
            id="channel-name"
            v-model="channelName"
            placeholder="general"
            @keyup.enter="handleCreate"
          />
        </div>
        <p v-if="error" class="text-sm text-destructive">{{ error }}</p>
        <Button class="w-full" :disabled="creating" @click="handleCreate">
          {{ creating ? 'Creating...' : 'Create Channel' }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
