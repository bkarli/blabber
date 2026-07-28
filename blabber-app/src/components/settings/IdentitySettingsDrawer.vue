<script setup lang="ts">
import { ref, onMounted, watch, nextTick } from 'vue';
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
import { Mic, Volume2, Trash2, PlayCircle } from 'lucide-vue-next';
import { api, type AudioDeviceInfo } from '@/lib/tauri';
import { useAuthStore } from '@/stores/auth';

const open = defineModel<boolean>('open', { default: false });

const auth = useAuthStore();

const endpointId = ref<string | null>(null);
const loadingEndpointId = ref(false);

const DEFAULT_DEVICE_VALUE = '__default__';

const inputDevices = ref<AudioDeviceInfo[]>([]);
const outputDevices = ref<AudioDeviceInfo[]>([]);
const selectedInput = ref<string>(DEFAULT_DEVICE_VALUE);
const selectedOutput = ref<string>(DEFAULT_DEVICE_VALUE);
const audioError = ref<string | null>(null);
const soundTestError = ref<string | null>(null);
const testingSound = ref(false);

// true while loadAudioDevices() is setting selectedInput/
// selectedOutput, so those watchers below don't re-send the value we just loaded
let suppressDeviceWatch = false;

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
  audioError.value = null;
  suppressDeviceWatch = true;
  try {
    const devices = await api.listAudioDevices();
    inputDevices.value = devices.inputs;
    outputDevices.value = devices.outputs;
    selectedInput.value = devices.selected_input ?? DEFAULT_DEVICE_VALUE;
    selectedOutput.value = devices.selected_output ?? DEFAULT_DEVICE_VALUE;
  } catch (e) {
    audioError.value = 'Failed to load audio devices.';
  }
  await nextTick();
  suppressDeviceWatch = false;
}

watch(open, (isOpen) => {
  if (isOpen) {
    loadEndpointId();
    loadAudioDevices();
  }
});

watch(selectedInput, async (name) => {
  if (suppressDeviceWatch) return;
  try {
    await api.setInputDevice(name === DEFAULT_DEVICE_VALUE ? null : name);
  } catch (e) {
    audioError.value = 'Failed to set input device.';
  }
});

watch(selectedOutput, async (name) => {
  if (suppressDeviceWatch) return;
  try {
    await api.setOutputDevice(name === DEFAULT_DEVICE_VALUE ? null : name);
  } catch (e) {
    audioError.value = 'Failed to set output device.';
  }
});

async function testSound() {
  soundTestError.value = null;
  testingSound.value = true;
  try {
    await api.playSoundEffect('test');
  } catch (e) {
    soundTestError.value = String(e);
  } finally {
    testingSound.value = false;
  }
}

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
                <SelectItem :value="DEFAULT_DEVICE_VALUE">System default</SelectItem>
                <SelectItem
                  v-for="device in inputDevices"
                  :key="device.name"
                  :value="device.name"
                >
                  {{ device.name }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div class="space-y-2">
            <Label class="flex items-center gap-2">
              <Volume2 class="h-4 w-4" /> Output device
            </Label>
            <div class="flex gap-2">
              <Select v-model="selectedOutput">
                <SelectTrigger class="flex-1">
                  <SelectValue placeholder="Select speaker" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem :value="DEFAULT_DEVICE_VALUE">System default</SelectItem>
                  <SelectItem
                    v-for="device in outputDevices"
                    :key="device.name"
                    :value="device.name"
                  >
                    {{ device.name }}
                  </SelectItem>
                </SelectContent>
              </Select>
              <Button variant="outline" size="icon" :disabled="testingSound" @click="testSound">
                <PlayCircle class="h-4 w-4" />
              </Button>
            </div>
          </div>

          <p v-if="audioError" class="text-sm text-destructive">
            {{ audioError }}
          </p>
          <p v-if="soundTestError" class="text-sm text-destructive">
            {{ soundTestError }}
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
