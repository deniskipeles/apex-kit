# Using ApexKit with Svelte

ApexKit works beautifully with Svelte and SvelteKit's reactive stores and server-side loading.

## Installation

```bash
npm install @apexkit/sdk
```

## Setup Client

```javascript
// src/lib/apexkit.js
import { ApexKit } from '@apexkit/sdk';
import { env } from '$env/dynamic/public';

const apex = new ApexKit(env.PUBLIC_APEXKIT_URL || 'http://localhost:5000');

export default apex;
```

## Authentication Store

```javascript
// src/lib/stores/auth.js
import { writable } from 'svelte/store';
import apex from '../apexkit';

export const user = writable(apex.auth.getUser());

export async function login(email, password) {
    const res = await apex.auth.login(email, password);
    user.set(res.user);
    return res;
}

export function logout() {
    apex.auth.logout();
    user.set(null);
}
```

## Fetching Data (SvelteKit)

In SvelteKit, you can fetch data in your `+page.js` or `+page.server.js`.

```javascript
// src/routes/posts/+page.js
import apex from '$lib/apexkit';

export async function load() {
    const result = await apex.collection('posts').list();
    return {
        posts: result.items
    };
}
```

Then in your `+page.svelte`:

```svelte
<script>
    export let data;
</script>

<h1>Posts</h1>
<ul>
    {#each data.posts as post}
        <li>{post.title}</li>
    {/each}
</ul>
```

## Realtime in Svelte

Svelte's `onMount` is perfect for establishing WebSocket connections.

```svelte
<script>
    import { onMount, onDestroy } from 'svelte';
    import { ApexKitRealtimeWSClient } from '@apexkit/sdk';
    import apex from '$lib/apexkit';

    let logs = [];
    let realtime;
    let unsubscribe;

    onMount(() => {
        realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
        realtime.connect();

        realtime.subscribe({ collectionId: 'logs' });

        unsubscribe = realtime.onEvent((msg) => {
            logs = [msg.payload.data, ...logs];
        });
    });

    onDestroy(() => {
        if (unsubscribe) unsubscribe();
        if (realtime) realtime.disconnect();
    });
</script>

{#each logs as log}
    <p>{log.message}</p>
{/each}
```
