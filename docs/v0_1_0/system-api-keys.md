# 🔑 Scoped & Composite API Key System

**Version:** 0.1.0  
**Context:** Authentication, Server-to-Server Communication, and API Access

ApexKit utilizes a hierarchical, scoped API key system paired with a composite database lookup strategy. This architecture ensures complete data isolation between tenants while allowing root operators to maintain secure global access, with $O(1)$ fast-fail validation.

---

## 1. Key Anatomy

Each API key is composed of five semantic segments separated by underscores, making them easily identifiable in logs and codebases:

$$\text{[Issuer]} \_ \text{[Target/Scope]} \_ \text{[Environment]} \_ \text{[Secret]} \_ \text{[Checksum]}$$

* **Secret:** A 64-character (32-byte) cryptographically secure random hex string.
* **Key ID:** The last 8 characters of the secret are extracted internally as a fast-lookup `key_id`.
* **Checksum:** A 4-character CRC derived from `SHA256(prefix_secret)` to prevent invalid keys from querying the database.

### Key Variants & Environments

#### Root-Issued Keys
* **Root-to-System Key (`sys`)**: Grants global administrative control over the master environment and all tenants.
  ```http
  root_sys_prod_5e884898da28047151d0e56f8dc6292773603d0d6aabbdd62a11ef721d1542d8_d7a8
  ```
* **Root-to-Tenant Key (`tnnt`)**: A root administrator acting safely on behalf of a specific tenant (e.g., `client-a`).
  ```http
  root_tnnt_client-a_prod_c4ca4238a0b923820dcc509a6f75849b..._f9b1
  ```

#### Tenant-Issued Keys
* **Tenant Secret Key (`sk`)**: Intended for secure Server-to-Server backend communication.
  ```http
  tnt_client-a_sk_prod_8f14e45fceea167a5a36dedd4bea2543..._a1b2
  ```
* **Tenant Public Key (`pk`)**: Intended for Client-side/Browser communication.
  ```http
  tnt_client-a_pk_prod_902ba3cda1883801594b6e1b452790cc..._c3d4
  ```

---

## 2. Security & Performance Architecture

This system addresses several critical architectural bottlenecks of multi-tenant API gateways:

### Fast-Fail Gateway Checksums
The last 4 characters of every key serve as a cryptographic checksum computed in memory.
* **Impact**: Malformed, incomplete, or brute-force key attempts are rejected immediately ($O(1)$ complexity) before initiating any database connection or heavy cryptographic hashing, protecting SQLite resources from exhaustion.

### Noisy-Neighbor Protected DB Queries
Traditional systems hash the entire key and query a monolithic global database, which creates index bottlenecks as the platform scales. ApexKit splits valid keys to extract the `tenant_id` and the fast-lookup `key_id`.
* **Database Query**: 
  ```sql
  SELECT secret_hash, roles, status, bypass_cors 
  FROM _api_keys_v2
  WHERE tenant_id = 'client-a' AND key_id = '1d1542d8' AND secret_hash = '...';
  ```
* **Impact**: The database lookup is scoped exclusively to that tenant's subset of keys ($O(\log N_{\text{tenant}})$), eliminating cross-tenant query latency.

### Automated Context Scoping
When a key is validated, the API gateway automatically injects its corresponding `tenant_id` directly into the HTTP request's execution extensions.
* **Impact**: This strictly binds the execution context (and `$db` object) to the target tenant’s isolated SQLite file, preventing accidental cross-tenant data leaks.

---

## 3. Permissions & CORS Integration

The `env_type` segment of the key dictates its CORS (Cross-Origin Resource Sharing) capabilities:

* **Public Keys (`pk`)**: Strict CORS enforcement. Even if `bypass_cors: true` is set in the database, ApexKit's security middleware overrides it for `pk` keys. They must strictly match your configured "Allowed Origins" whitelist in the Root dashboard.
* **Secret Keys (`sk` / `sys`)**: Flexible CORS. If `bypass_cors: true` is configured, these keys instruct the middleware to bypass origin checks, allowing native mobile apps, desktop apps, or server-to-server microservices to interact with the API seamlessly.

---

## 4. Usage Guide

To authenticate requests using an API key, attach it via the **`x-api-key`** header. 

*(Note: Standard JWTs use the `Authorization: Bearer` header, but API Keys must specifically use `x-api-key`).*

### cURL Example
```bash
curl -X GET "https://api.your-app.com/api/v1/collections/products/records" \
  -H "x-api-key: tnt_client-a_sk_prod_8f14e45f..." \
  -H "Content-Type: application/json"
```

### Native Fetch API Example
```javascript
const response = await fetch("https://api.your-app.com/api/v1/collections/products/records", {
  method: "GET",
  headers: {
    "x-api-key": "tnt_client-a_sk_prod_8f14e45f...",
    "Content-Type": "application/json"
  }
});

const data = await response.json();
console.log(data.items);
```

### TypeScript SDK (`@apexkit/sdk`) Example
In the JavaScript/TypeScript SDK, you configure the API key globally by injecting it into the custom headers map:

```typescript
import { ApexKit } from '@apexkit/sdk';

// 1. Initialize the client
const apex = new ApexKit("https://api.your-app.com");

// 2. Authenticate using the API Key
apex.setHeader('x-api-key', 'tnt_client-a_sk_prod_8f14e45f...');

// 3. All subsequent requests will automatically use the API Key
const products = await apex.collection("products").list({
  page: 1,
  per_page: 20,
  sort: "-created"
});

console.log(products.total);
```
