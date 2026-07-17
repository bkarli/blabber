<script setup lang="ts">
import { ref } from 'vue';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { useAppStore } from '@/stores/app';
import { useRouter } from 'vue-router';

const open = defineModel<boolean>('open', { default: false });
const props = defineProps<{ spaceId: string }>();

const store = useAppStore();
const router = useRouter();

const roomName = ref('');
const creating = ref(false);
const error = ref<string | null>(null);

async function handleCreate() {
  error.value = null;
  if (!roomName.value.trim()) {
    error.value = 'Room name is required.';
    return;
  }
  creating.value = true;
  try {
    const room = await store.createRoom(props.spaceId, roomName.value.trim());
    roomName.value = '';
    open.value = false;
    router.push({ name: 'room', params: { spaceId: props.spaceId, roomId: room.id } });
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
        <DialogTitle>Create a room</DialogTitle>
      </DialogHeader>

      <div class="space-y-4 pt-2">
        <div class="space-y-2">
          <Label for="room-name">Room name</Label>
          <Input
            id="room-name"
            v-model="roomName"
            placeholder="general"
            @keyup.enter="handleCreate"
          />
        </div>
        <p v-if="error" class="text-sm text-destructive">{{ error }}</p>
        <Button class="w-full" :disabled="creating" @click="handleCreate">
          {{ creating ? 'Creating...' : 'Create Room' }}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
