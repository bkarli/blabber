<script setup lang="ts">
import { ref } from 'vue';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Send } from 'lucide-vue-next';
import { useAppStore } from '@/stores/app';

const props = defineProps<{ spaceId: string; roomId: string }>();
const store = useAppStore();

const content = ref('');
const sending = ref(false);

async function send() {
  if (!content.value.trim() || sending.value) return;
  sending.value = true;
  const toSend = content.value.trim();
  content.value = '';
  try {
    await store.sendMessage(props.spaceId, props.roomId, toSend);
  } catch (e) {
    console.error('failed to send message', e);
    content.value = toSend; // restore on failure
  } finally {
    sending.value = false;
  }
}
</script>

<template>
  <div class="h-16 flex gap-2 border-t border-border p-3">
    <Input
      v-model="content"
      placeholder="Message..."
      :disabled="sending"
      @keyup.enter="send"
    />
    <Button size="icon" :disabled="sending || !content.trim()" @click="send">
      <Send class="h-4 w-4" />
    </Button>
  </div>
</template>
