# Example: IoT Sensor Dashboard

A system to track temperature, humidity, and status from thousands of IoT devices in real-time.

## 1. Database Collections

### `devices`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `serial_number` | Text | Unique | |
| `type` | Text | | `thermometer`, `hygrometer` |
| `location` | JSON | | `{ lat, lng }` |

### `telemetry`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `device_id` | Relation | Required | Collection: `devices` |
| `value` | Number | Required | |
| `unit` | Text | | `C`, `%`, `Pa` |

## 2. Security Policies

- **`devices`**:
  - `read`: `auth`
  - `create`: `admin`
- **`telemetry`**:
  - `create`: `auth` (Device API Key)
  - `read`: `auth`

## 3. Handling High-Frequency Data

Devices can send data via the REST API or a specialized Edge Function.

### `ingest-telemetry` Edge Function
```javascript
export default async function(req) {
    const { serial, value } = await req.json();

    // Find device
    const device = await $db.records.get('devices', { serial_number: serial });

    // Log telemetry
    await $db.records.create('telemetry', {
        device_id: device.id,
        value: value,
        created: new Date().toISOString()
    });

    // Check for alerts
    if (value > 40) {
        await $realtime.broadcast('alerts', 'overheat', { device: serial, temp: value });
    }

    return new Response({ ok: true });
}
```

## 4. Real-time Monitoring (Frontend)

```javascript
const realtime = new ApexKitRealtimeWSClient(apex.baseUrl, apex.getToken());
realtime.connect();

realtime.subscribe({ collectionId: 'telemetry' });

realtime.onEvent((msg) => {
    updateChart(msg.payload.data.device_id, msg.payload.data.value);
});
```
