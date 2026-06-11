# ApexKit Scoped & Composite API Key System

ApexKit utilizes a hierarchical, scoped API key system paired with a composite database lookup. This architecture ensures complete data isolation between tenants while allowing root operators to maintain secure global access.

---

## 1. Key Anatomy

Each API key is composed of five cryptographic and identifying segments separated by underscores:

$$\text{[Issuer]} \_ \text{[Target/Scope]} \_ \text{[Environment]} \_ \text{[Secret]} \_ \text{[Checksum]}$$

### Key Variants

#### Root-Issued Keys
*   **Root-to-System Key** (Global administrative control over all environments):
    ```http
    root_sys_prod_7fH3k9a1b2c3d4e5f6g7h8i9j0k1l2m3_ad82
    ```
*   **Root-to-Tenant Key** (Root administrator acting on behalf of a specific tenant):
    ```http
    root_tnnt_customer1_prod_7fH3k9a1b2c3d4e5f6g7h8i9j0k1l2m3_ad82
    ```

#### Tenant-Issued Keys
*   **Tenant Secret Key** (Server-to-Server communication; bypasses CORS restrictions):
    ```http
    tnt_customer1_sk_prod_2xP8n1a1b2c3d4e5f6g7h8i9j0k1l2m3_fb93
    ```
*   **Tenant Public Key** (Client-side/browser communication; restricted by CORS origin whitelists):
    ```http
    tnt_customer1_pk_prod_9wM4k2a1b2c3d4e5f6g7h8i9j0k1l2m3_cc12
    ```

---

## 2. Security & Performance Architecture

This system addresses several architectural concerns of multi-tenant gateways:

### Fast-Fail Gateway Checksums
The last 4 characters of every key serve as a cryptographic checksum computed from `sha256(prefix + secret)`. When a request reaches the ApexKit API gateway, the router validates this checksum locally in-memory.
*   **Performance Impact**: Invalid or malformed keys are rejected immediately ($O(1)$ complexity) before initiating any database connections or cryptographic work, protecting SQLite resources from brute-force exhaustion.

### Noisy-Neighbor Protected DB Queries
Traditional systems hash the entire key and query a global database, which can lead to index bottlenecks as the platform scales. ApexKit splits valid keys to extract the `tenant_id` and the fast-lookup `key_id` (the last 8 characters of the secret).
*   **Database Query**: 
    ```sql
    SELECT secret_hash, roles, status, bypass_cors 
    FROM _api_keys 
    WHERE tenant_id = 'customer1' AND key_id = 'l2m3';
    ```
*   **Performance Impact**: The database lookup is scoped exclusively to that tenant's key records ($O(\log N_{\text{tenant}})$), eliminating cross-tenant query latency.

### Automated Context Scoping
When a key is validated, the gateway automatically injects its corresponding `tenant_id` or `sandbox_id` directly into the request's execution thread.
*   **Security Impact**: This binds the execution context to the tenant’s SQLite partition, preventing data bleeding or accidental cross-tenant modifications.

---

## 3. Key Permissions & CORS

*   **Public Keys (`pk_prod`)**: Intended strictly for client-side environments (such as mobile apps or web browsers). These keys are subject to strict CORS origin verification and are limited to public-facing actions (such as anonymous inserts or authentication requests).
*   **Secret Keys (`sk_prod`)**: Intended for secure backend services. These keys can be configured with `bypass_cors: true` to bypass origin checks during server-to-server operations.

---

## 4. Usage Guide

To use an API key, attach it to your HTTP requests using either the `x-api-key` header or the standard `Authorization` Bearer header.

### cURL Example
```bash
curl -X GET "https://api.apexkit.io/api/v1/collections/products/records" \
  -H "x-api-key: tnt_customer1_sk_prod_2xP8n1a1b2c3d4e5f6g7h8i9j0k1l2m3_fb93" \
  -H "Content-Type: application/json"
```

### JavaScript Fetch Example
```javascript
const response = await fetch("https://api.apexkit.io/api/v1/collections/products/records", {
  method: "GET",
  headers: {
    "x-api-key": "tnt_customer1_sk_prod_2xP8n1a1b2c3d4e5f6g7h8i9j0k1l2m3_fb93",
    "Content-Type": "application/json"
  }
});

const data = await response.json();
console.log(data.items);
```

### TypeScript SDK Example
```typescript
import { ApexKit } from '@apexkit/sdk';

// Initialize the client. It automatically detects and uses the API key
const client = new ApexKit("https://api.apexkit.io");
client.setToken("tnt_customer1_sk_prod_2xP8n1a1b2c3d4e5f6g7h8i9j0k1l2m3_fb93");

const products = await client.collection("products").list({
  page: 1,
  per_page: 20
});
```