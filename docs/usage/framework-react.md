# Using ApexKit with React

Integrating ApexKit into a React application is seamless. This guide covers setup, hooks, and best practices.

## Installation

```bash
npm install @apexkit/sdk
```

## Setup Client

Create a `src/lib/apexkit.js` file to initialize the client.

```javascript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit(import.meta.env.VITE_APEXKIT_URL || 'http://localhost:5000');

export default apex;
```

## Authentication Provider

A common pattern is to wrap your app in an Auth context.

```jsx
// src/context/AuthContext.jsx
import { createContext, useContext, useState, useEffect } from 'react';
import apex from '../lib/apexkit';

const AuthContext = createContext();

export const AuthProvider = ({ children }) => {
  const [user, setUser] = useState(apex.auth.getUser());

  const login = async (email, password) => {
    const res = await apex.auth.login(email, password);
    setUser(res.user);
    return res;
  };

  const logout = () => {
    apex.auth.logout();
    setUser(null);
  };

  return (
    <AuthContext.Provider value={{ user, login, logout }}>
      {children}
    </AuthContext.Provider>
  );
};

export const useAuth = () => useContext(AuthContext);
```

## Fetching Data

You can use standard `useEffect` or libraries like `TanStack Query` (recommended).

### Using TanStack Query

```jsx
import { useQuery } from '@tanstack/react-query';
import apex from './lib/apexkit';

function PostList() {
  const { data, isLoading } = useQuery({
    queryKey: ['posts'],
    queryFn: () => apex.collection('posts').list({ sort: '-created' })
  });

  if (isLoading) return <div>Loading...</div>;

  return (
    <ul>
      {data.items.map(post => (
        <li key={post.id}>{post.title}</li>
      ))}
    </ul>
  );
}
```

## Realtime Updates

ApexKit's WebSockets work great with React's `useEffect`.

```jsx
import { useEffect, useState } from 'react';
import { ApexKitRealtimeWSClient } from '@apexkit/sdk';
import apex from './lib/apexkit';

function LivePosts() {
  const [posts, setPosts] = useState([]);

  useEffect(() => {
    const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
    realtime.connect();

    realtime.subscribe({ collectionId: 'posts' });

    const unsubscribe = realtime.onEvent((msg) => {
      if (msg.event === 'Insert') {
        setPosts(prev => [msg.payload.data, ...prev]);
      }
    });

    return () => {
      unsubscribe();
      realtime.disconnect();
    };
  }, []);

  return (
    <div>
      {posts.map(post => <div key={post.id}>{post.title}</div>)}
    </div>
  );
}
```

## Next.js Integration

For Next.js, ensure you only initialize the SDK on the client side or use environment variables correctly for SSR.

```javascript
// lib/apexkit.js
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit(process.env.NEXT_PUBLIC_APEXKIT_URL);

export default apex;
```

In `getServerSideProps`, you can use the SDK to fetch data:

```javascript
export async function getServerSideProps() {
  const posts = await apex.collection('posts').list();
  return { props: { posts: posts.items } };
}
```
