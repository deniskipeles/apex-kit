# Example: Digital Asset Management (DAM)

A centralized hub for storing, organizing, and transforming company media assets.

## 1. Database Collections

### `assets`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `name` | Text | Required | |
| `file` | File | Required | |
| `folder` | Text | | `/marketing`, `/product` |
| `tags` | JSON | | List of strings |
| `version` | Number | Default: `1` | |

## 2. Security Policies

- **`assets`**:
  - `read`: `auth`
  - `create/update`: `auth.role == 'uploader'`
  - `delete`: `admin`

## 3. Automatic Metadata Extraction

### `extract-metadata`
**Trigger**: After Create on `assets`
```javascript
export default async function(req) {
    const fileUrl = $fs.getUrl(req.record.file);

    // Get AI to tag the image
    const tags = await $ai.run('tag-image', { url: fileUrl });

    await $db.records.patch('assets', req.record.id, {
        tags: tags
    });
}
```

## 4. Serving Transformed Images

```javascript
// Display a 500px wide WebP version of the asset
const asset = await apex.collection('assets').get('asset123');
const imageUrl = apex.files.getFileUrl(asset.file) + '?width=500&format=webp';

document.getElementById('preview').src = imageUrl;
```

## 5. Folder Search

```javascript
const marketingAssets = await apex.collection('assets').list({
    filter: { folder: '/marketing' },
    sort: '-created'
});
```
