<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import {
  Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription, SheetFooter, SheetClose,
} from '@/components/ui/sheet';
import {
  AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription,
  AlertDialogFooter, AlertDialogCancel, AlertDialogAction,
} from '@/components/ui/alert-dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Label } from '@/components/ui/label';
import { Separator } from '@/components/ui/separator';
import { Button } from '@/components/ui/button';
import { Mic, Volume2, Trash2 } from 'lucide-vue-next';
import { api } from '@/lib/tauri';
import { useAuthStore } from '@/stores/auth';
import {
  selectedInputId as selectedInput,
  selectedOutputId as selectedOutput,
  selectedInputLabel,
  selectedOutputLabel,
} from '@/components/settings/useAudioSettings';
const open = defineModel<boolean>('open', { default: false });

const auth = useAuthStore();

const endpointId = ref<string | null>(null);
const loadingEndpointId = ref(false);

const inputDevices = ref<MediaDeviceInfo[]>([]);
const outputDevices = ref<MediaDeviceInfo[]>([]);
const devicePermissionError = ref<string | null>(null);
const confirmDeleteOpen = ref(false);
const deleting = ref(false);
const deleteError = ref<string | null>(null);

async function loadEndpointId() {
  loadingEndpointId.value = true;
  try {
    endpointId.value = await api.myEndpointId();
  } catch (e) {
    endpointId.value = null;
  } finally {
    loadingEndpointId.value = false;
  }
}

async function loadAudioDevices() {
  devicePermissionError.value = null;
  try {
    const tempStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    tempStream.getTracks().forEach((track) => track.stop());

    const devices = await navigator.mediaDevices.enumerateDevices();
    inputDevices.value = devices.filter((d) => d.kind === 'audioinput');
    outputDevices.value = devices.filter((d) => d.kind === 'audiooutput');

    if (inputDevices.value.length > 0 && !selectedInput.value) {
      selectedInput.value = inputDevices.value[0].deviceId;
    }
    if (outputDevices.value.length > 0 && !selectedOutput.value) {
      selectedOutput.value = outputDevices.value[0].deviceId;
    }
  } catch (e) {
    console.error('getUserMedia failed:', e);
    devicePermissionError.value = 'Microphone access is required to list audio devices.';
  }
}

watch(open, (isOpen) => {
  if (isOpen) {
    loadEndpointId();
    loadAudioDevices();
  }
});

// cpal (the call audio backend) selects devices by name, so keep the persisted
// label in sync with whichever deviceId is currently selected.
watch([selectedInput, inputDevices], ([id, devices]) => {
  const match = devices.find((d) => d.deviceId === id);
  if (match) selectedInputLabel.value = match.label;
});
watch([selectedOutput, outputDevices], ([id, devices]) => {
  const match = devices.find((d) => d.deviceId === id);
  if (match) selectedOutputLabel.value = match.label;
});

async function handleDelete() {
  if (!auth.displayName) return;
  deleteError.value = null;
  deleting.value = true;
  try {
    await api.deleteIdentity(auth.displayName);
    confirmDeleteOpen.value = false;
    open.value = false;
    auth.displayName = null;
    window.location.reload();
  } catch (e) {
    deleteError.value = String(e);
  } finally {
    deleting.value = false;
  }
}
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent side="right" class="flex w-full flex-col sm:max-w-md">
      <SheetHeader>
        <SheetTitle>Identity Settings</SheetTitle>
        <SheetDescription>Manage your audio devices and identity.</SheetDescription>
      </SheetHeader>

      <div class="flex-1 space-y-6 overflow-y-auto px-4 py-2">
        <div class="space-y-4">
          <h3 class="text-sm font-semibold text-muted-foreground">Audio</h3>

          <div class="space-y-2">
            <Label class="flex items-center gap-2">
              <Mic class="h-4 w-4" /> Input device
            </Label>
            <Select v-model="selectedInput">
              <SelectTrigger>
                <SelectValue placeholder="Select microphone" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem
                  v-for="device in inputDevices"
                  :key="device.deviceId"
                  :value="device.deviceId"
                >
                  {{ device.label || 'Unknown microphone' }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-2">
            <Label class="flex items-center gap-2">
              <Volume2 class="h-4 w-4" /> Output device
            </Label>
            <Select v-model="selectedOutput">
              <SelectTrigger>
                <SelectValue placeholder="Select speaker" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem
                  v-for="device in outputDevices"
                  :key="device.deviceId"
                  :value="device.deviceId"
                >
                  {{ device.label || 'Unknown speaker' }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <p v-if="devicePermissionError" class="text-sm text-destructive">
            {{ devicePermissionError }}
          </p>
        </div>

        <Separator />

        <div class="space-y-2">
          <h3 class="text-sm font-semibold text-muted-foreground">Identity</h3>
          <div class="flex justify-between text-sm">
            <span class="text-muted-foreground">Display name</span>
            <span class="font-medium">{{ auth.displayName }}</span>
          </div>
          <div class="flex justify-between text-sm">
            <span class="text-muted-foreground">Endpoint ID</span>
            <span class="max-w-[220px] truncate font-mono text-xs">
              {{ loadingEndpointId ? 'Loading...' : (endpointId ?? 'Unavailable') }}
            </span>
          </div>
        </div>

        <Separator />

        <div class="space-y-2">
          <h3 class="text-sm font-semibold text-destructive">Danger Zone</h3>
          <Button variant="destructive" class="w-full" @click="confirmDeleteOpen = true">
            <Trash2 class="mr-2 h-4 w-4" />
            Delete Identity
          </Button>
        </div>
      </div>

      <SheetFooter>
        <SheetClose as-child>
          <Button variant="outline">Close</Button>
        </SheetClose>
      </SheetFooter>
    </SheetContent>
  </Sheet>

  <AlertDialog v-model:open="confirmDeleteOpen">
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>Delete this identity?</AlertDialogTitle>
        <AlertDialogDescription>
          This will permanently delete "{{ auth.displayName }}" from this device.
          This action cannot be undone, and you will lose access to this identity's
          spaces unless you have another way to rejoin them.
        </AlertDialogDescription>
      </AlertDialogHeader>
      <p v-if="deleteError" class="text-sm text-destructive">{{ deleteError }}</p>
      <AlertDialogFooter>
        <AlertDialogCancel :disabled="deleting">Cancel</AlertDialogCancel>
        <AlertDialogAction
          class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          :disabled="deleting"
          @click.prevent="handleDelete"
        >
          {{ deleting ? 'Deleting...' : 'Delete Identity' }}
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
