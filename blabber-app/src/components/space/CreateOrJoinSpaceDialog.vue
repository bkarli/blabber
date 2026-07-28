<script setup lang="ts">
import { ref } from 'vue';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { useAppStore } from '@/stores/app';
import { useRouter } from 'vue-router';

const open = defineModel<boolean>('open', { default: false });

const store = useAppStore();
const router = useRouter();

const newSpaceName = ref('');
const creating = ref(false);
const createError = ref<string | null>(null);

const inviteTicket = ref('');
const joining = ref(false);
const joinError = ref<string | null>(null);

async function handleCreate() {
  createError.value = null;
  if (!newSpaceName.value.trim()) {
    createError.value = 'Space name is required.';
    return;
  }
  creating.value = true;
  try {
    const info = await store.createSpace(newSpaceName.value.trim());
    newSpaceName.value = '';
    open.value = false;
    router.push({ name: 'space', params: { spaceId: info.id } });
  } catch (e) {
    createError.value = String(e);
  } finally {
    creating.value = false;
  }
}

async function handleJoin() {
  joinError.value = null;
  if (!inviteTicket.value.trim()) {
    joinError.value = 'Invite code is required.';
    return;
  }
  joining.value = true;
  try {
    const info = await store.joinSpace(inviteTicket.value.trim());
    inviteTicket.value = '';
    open.value = false;
    router.push({ name: 'space', params: { spaceId: info.id } });
  } catch (e) {
    joinError.value = String(e);
  } finally {
    joining.value = false;
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>Add a space</DialogTitle>
      </DialogHeader>

      <Tabs default-value="create" class="w-full">
        <TabsList class="grid w-full grid-cols-2">
          <TabsTrigger value="create">Create</TabsTrigger>
          <TabsTrigger value="join">Join</TabsTrigger>
        </TabsList>

        <TabsContent value="create" class="space-y-4 pt-4">
          <div class="space-y-2">
            <Label for="space-name">Space name</Label>
            <Input
              id="space-name"
              v-model="newSpaceName"
              placeholder="My Space"
              @keyup.enter="handleCreate"
            />
          </div>
          <p v-if="createError" class="text-sm text-destructive">{{ createError }}</p>
          <Button class="w-full" :disabled="creating" @click="handleCreate">
            {{ creating ? 'Creating...' : 'Create Space' }}
          </Button>
        </TabsContent>

        <TabsContent value="join" class="space-y-4 pt-4">
          <div class="space-y-2">
            <Label for="invite-ticket">Invite code</Label>
          <Textarea
            id="invite-ticket"
            v-model="inviteTicket"
            placeholder="Paste an invite code..."
            class="h-24 resize-none whitespace-pre-wrap break-all font-mono text-xs"
          />
          </div>
          <p v-if="joinError" class="text-sm text-destructive">{{ joinError }}</p>
          <Button class="w-full" :disabled="joining" @click="handleJoin">
            {{ joining ? 'Joining...' : 'Join Space' }}
          </Button>
        </TabsContent>
      </Tabs>
    </DialogContent>
  </Dialog>
</template>
