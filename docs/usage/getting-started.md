# Getting Started with ApexKit

This guide will walk you through setting up ApexKit and making your first API call.

## 1. Installation

### Using Docker (Recommended)

The fastest way to get started is using Docker.

```bash
docker run -p 5000:5000 \
  -v /path/to/local/data:/app/storage \
  deniskipeles/apexkit:latest
```

### Pre-compiled Binaries

Download the binary for your platform from the [GitHub Releases](https://github.com/deniskipeles/apexkit/releases) page.

```bash
# macOS/Linux
chmod +x apexkit
./apexkit --port 5000

# Windows
apexkit.exe --port 5000
```

## 2. Access the Admin Dashboard

Once the server is running, open your browser and navigate to:
`http://localhost:5000/_dashboard`

**Default Credentials:**
- **Email**: `admin@apexkit.io`
- **Password**: `password`

*Note: Please change the password immediately after your first login.*

## 3. Create Your First Collection

1. Go to the **Collections** tab.
2. Click **New Collection**.
3. Name it `posts`.
4. Add the following fields:
   - `title` (Text)
   - `content` (Rich Text)
   - `status` (Select: draft, published)
5. Click **Save**.

ApexKit has now instantly created REST and GraphQL endpoints for your `posts` collection.

## 4. Install the SDK

In your frontend project, install the ApexKit SDK:

```bash
npm install @apexkit/sdk
```

## 5. Initialize the Client

```javascript
import { ApexKit } from '@apexkit/sdk';

const apex = new ApexKit('http://localhost:5000');

// Create a record
async function createPost() {
  const post = await apex.collection('posts').create({
    title: "My First Post",
    content: "ApexKit is amazing!"
  });
  console.log(post);
}

createPost();
```

## 6. Security and Policies

By default, new collections are restricted. To allow public reading:

1. Click on your `posts` collection.
2. Navigate to **Access Control / Policies**.
3. Set the `read` policy to `public`.
4. Click **Save**.

Now anyone can fetch your posts without an API key or authentication.

## Next Steps

- Explore the **[SDK Reference](./javascript-sdk.md)**.
- Learn about **[Edge Functions](./edge-functions.md)** for custom logic.
- See how to use **[GraphQL](./graphql-api.md)** with ApexKit.
