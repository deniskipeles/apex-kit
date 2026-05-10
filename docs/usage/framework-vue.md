# Using ApexKit with Vue

This guide explains how to integrate ApexKit into a Vue 3 application using the Composition API.

## Installation

```bash
npm install @apexkit/sdk
```

## Setup Client

Create a utility file to export the ApexKit instance.

```javascript
// src/lib/apexkit.js
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit(import.meta.env.VITE_APEXKIT_URL || 'http://localhost:5000');

export default apex;
```

## Global Authentication Store (Pinia)

Using Pinia is the recommended way to manage auth state in Vue.

```javascript
// src/stores/auth.js
import { defineStore } from 'pinia';
import apex from '../lib/apexkit';

export const useAuthStore = defineStore('auth', {
  state: () => ({
    user: apex.auth.getUser(),
  }),
  actions: {
    async login(email, password) {
      const res = await apex.auth.login(email, password);
      this.user = res.user;
      return res;
    },
    logout() {
      apex.auth.logout();
      this.user = null;
    }
  }
});
```

## Fetching Data in Components

Use `onMounted` and `ref` to handle data fetching.

```vue
<script setup>
import { ref, onMounted } from 'vue';
import apex from '../lib/apexkit';

const posts = ref([]);
const loading = ref(true);

onMounted(async () => {
  try {
    const result = await apex.collection('posts').list();
    posts.value = result.items;
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <div v-if="loading">Loading...</div>
  <ul v-else>
    <li v-for="post in posts" :key="post.id">
      {{ post.title }}
    </li>
  </ul>
</template>
```

## Realtime Subscriptions

```javascript
<script setup>
import { ref, onMounted, onUnmounted } from 'vue';
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';
import apex from '../lib/apexkit';

const messages = ref([]);
let realtime = null;
let unsubscribe = null;

onMounted(() => {
  realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
  realtime.connect();

  realtime.subscribe({ channel: 'global_chat' });

  unsubscribe = realtime.onEvent((msg) => {
    if (msg.event === 'Custom') {
      messages.value.push(msg.payload.data);
    }
  });
});

onUnmounted(() => {
  if (unsubscribe) unsubscribe();
  if (realtime) realtime.disconnect();
});
</script>
```

## Nuxt Integration

In Nuxt 3, you can create a plugin to provide ApexKit globally.

```javascript
// plugins/apexkit.client.js
import { ApexKit } from '@apexkit/sdk';

export default defineNuxtPlugin(() => {
  const config = useRuntimeConfig();
  const apex = new ApexKit(config.public.apexkitUrl);

  return {
    provide: {
      apex
    }
  };
});
```
