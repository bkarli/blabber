<script setup lang="ts">
import { ref } from 'vue';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Send, Image as ImageIcon, Paperclip } from 'lucide-vue-next';
import { open } from '@tauri-apps/plugin-dialog';
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
    content.value = toSend;
  } finally {
    sending.value = false;
  }
}

async function pickImage() {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }],
  });
  if (!selected) return;

  try {
    await store.sendImage(props.spaceId, props.roomId, selected as string);
  } catch (e) {
    console.error('failed to send image', e);
  }
}

async function pickFile() {
  const selected = await open({ multiple: false }); // no filter — any file type
  if (!selected) return;

  try {
    await store.sendFile(props.spaceId, props.roomId, selected as string);
  } catch (e) {
    console.error('failed to send file', e);
  }
}
</script>

<template>
  <div class="flex h-16 gap-2 border-t border-border p-3">
    <Button variant="ghost" size="icon" :disabled="sending" @click="pickImage">
      <ImageIcon class="h-4 w-4" />
    </Button>
    <Button variant="ghost" size="icon" :disabled="sending" @click="pickFile">
      <Paperclip class="h-4 w-4" />
    </Button>
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
