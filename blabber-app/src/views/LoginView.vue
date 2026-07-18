<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/tauri';
import { useAuthStore } from '@/stores/auth';

const auth = useAuthStore();

// --- create identity ---
const newDisplayName = ref('');
const newPassword = ref('');
const createError = ref<string | null>(null);
const creating = ref(false);

async function handleCreate() {
  createError.value = null;
  if (!newDisplayName.value.trim() || !newPassword.value) {
    createError.value = 'Display name and password are required.';
    return;
  }
  creating.value = true;
  try {
    await auth.createIdentity(newDisplayName.value.trim(), newPassword.value);
  } catch (e) {
    createError.value = String(e);
  } finally {
    creating.value = false;
  }
}

// --- login with existing identity ---
const identities = ref<string[]>([]);
const selectedIdentity = ref('');
const loginPassword = ref('');
const loginError = ref<string | null>(null);
const loggingIn = ref(false);

onMounted(async () => {
  try {
    identities.value = await api.listIdentities();
    if (identities.value.length > 0) {
      selectedIdentity.value = identities.value[0];
    }
  } catch (e) {
    loginError.value = String(e);
  }
});

async function handleLogin() {
  loginError.value = null;
  if (!selectedIdentity.value || !loginPassword.value) {
    loginError.value = 'Select an identity and enter your password.';
    return;
  }
  loggingIn.value = true;
  try {
    await auth.login(selectedIdentity.value, loginPassword.value);
  } catch (e) {
    loginError.value = String(e);
  } finally {
    loggingIn.value = false;
  }
}
</script>

<template>
  <div class="flex min-h-screen items-center justify-center bg-background p-4">
    <Card class="w-full max-w-sm">
      <CardHeader>
        <CardTitle>Welcome to Blabber</CardTitle>
        <CardDescription>Create a new identity or log in to an existing one.</CardDescription>
      </CardHeader>
      <CardContent>
        <Tabs default-value="login" class="w-full">
          <TabsList class="grid w-full grid-cols-2">
            <TabsTrigger value="login">Login</TabsTrigger>
            <TabsTrigger value="create">Create Identity</TabsTrigger>
          </TabsList>

          <TabsContent value="login" class="space-y-4 pt-4">
            <div v-if="identities.length === 0" class="text-sm text-muted-foreground">
              No identities found on this device yet. Create one instead.
            </div>
            <template v-else>
              <div class="space-y-2">
                <Label for="identity-select">Identity</Label>
                <Select v-model="selectedIdentity">
                  <SelectTrigger id="identity-select">
                    <SelectValue placeholder="Select an identity" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem v-for="name in identities" :key="name" :value="name">
                      {{ name }}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div class="space-y-2">
                <Label for="login-password">Password</Label>
                <Input
                  id="login-password"
                  v-model="loginPassword"
                  type="password"
                  @keyup.enter="handleLogin"
                />
              </div>
              <p v-if="loginError" class="text-sm text-destructive">{{ loginError }}</p>
              <Button class="w-full" :disabled="loggingIn" @click="handleLogin">
                {{ loggingIn ? 'Logging in...' : 'Login' }}
              </Button>
            </template>
          </TabsContent>

          <TabsContent value="create" class="space-y-4 pt-4">
            <div class="space-y-2">
              <Label for="new-display-name">Display Name</Label>
              <Input id="new-display-name" v-model="newDisplayName" placeholder="Alice" />
            </div>
            <div class="space-y-2">
              <Label for="new-password">Password</Label>
              <Input
                id="new-password"
                v-model="newPassword"
                type="password"
                @keyup.enter="handleCreate"
              />
            </div>
            <p v-if="createError" class="text-sm text-destructive">{{ createError }}</p>
            <Button class="w-full" :disabled="creating" @click="handleCreate">
              {{ creating ? 'Creating...' : 'Create Identity' }}
            </Button>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  </div>
</template>
