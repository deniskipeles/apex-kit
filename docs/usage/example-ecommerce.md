# Example: Multi-tenant E-commerce Platform

A platform like Shopify where multiple vendors can create their own stores with unique products and orders.

## 1. Database Collections

### `stores` (Root-level)
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `name` | Text | Required | |
| `subdomain` | Text | Unique | |
| `tenant_id` | Text | Unique | Links to ApexKit Tenant |

### `products` (Tenant-scoped)
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `name` | Text | Required | |
| `price` | Number | Required | |
| `inventory` | Number | | |
| `image` | File | | |

### `orders` (Tenant-scoped)
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `customer_email` | Text | Required | |
| `total` | Number | | |
| `items` | JSON | | List of product IDs and quantities |

## 2. Security Policies

- **`products`**:
  - `read`: `public`
  - `create/update/delete`: `admin` (Store manager)
- **`orders`**:
  - `create`: `public`
  - `read`: `owner:customer_email` (or authenticated customer)

## 3. Connecting via Tenant SDK

When a user visits `vendor1.market.com`, the frontend identifies the `tenant_id`.

```javascript
const storeId = await getTenantIdFromSubdomain(window.location.host);
const storeClient = apex.tenant(storeId);

// Fetch products for this specific store
const products = await storeClient.collection('products').list();
```

## 4. Edge Functions

### `process-payment`
**Trigger**: HTTP POST
```javascript
export default async function(req) {
    const { orderId, stripeToken } = await req.json();

    const order = await $db.records.get('orders', orderId);

    // Call Stripe API
    const charge = await $http.post('https://api.stripe.com/v1/charges', {
        amount: order.total * 100,
        currency: 'usd',
        source: stripeToken
    }, {
        headers: { 'Authorization': `Bearer ${$config.STRIPE_SECRET}` }
    });

    if (charge.status === 'succeeded') {
        await $db.records.patch('orders', orderId, { status: 'paid' });
    }
}
```
