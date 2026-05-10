# Example: Portfolio Website with CMS

A personal website where the frontend is hosted on ApexKit and the content is managed via the dashboard.

## 1. Database Collections

### `projects`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `title` | Text | Required | |
| `description` | Text | | |
| `link` | Text | | |
| `image` | File | | |
| `order` | Number | | |

## 2. Security Policies

- **`projects`**:
  - `read`: `public`
  - `create/update/delete`: `admin`

## 3. Deploying the Frontend

ApexKit can host your static files (HTML/JS/CSS).

1. Build your React/Vue/Svelte project: `npm run build`.
2. Zip the `dist` folder.
3. Upload it via the **Admin > Site > Deploy** tab or via the CLI.

```bash
./apexkit site deploy ./dist.zip
```

## 4. Fetching Content (SDK)

In your portfolio's `App.jsx`:

```javascript
import apex from './lib/apexkit';

function Portfolio() {
  const [projects, setProjects] = useState([]);

  useEffect(() => {
    apex.collection('projects').list({ sort: 'order' })
      .then(res => setProjects(res.items));
  }, []);

  return (
    <div>
      {projects.map(p => (
        <ProjectCard key={p.id} data={p} />
      ))}
    </div>
  );
}
```

## 5. Contact Form with Email

### `contact-form`
**Trigger**: HTTP POST /run/contact
```javascript
export default async function(req) {
    const { name, email, message } = await req.json();

    await $util.sendEmail({
        to: 'your-email@example.com',
        subject: `New Portfolio Message from ${name}`,
        body: `From: ${email}\n\n${message}`
    });

    return new Response({ success: true });
}
```
