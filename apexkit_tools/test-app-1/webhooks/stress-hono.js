/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "id": 11,
  "name": "stress-hono",
  "extension": "js",
  "target_collection": null,
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "manual",
  "active": true,
  "visibility": "private"
};

/** @jsxImportSource https://esm.sh/hono/jsx */
import { Hono } from "https://esm.sh/hono";

const app = new Hono();

const Layout = (props: any) => {
  return (
    <html>
      <head>
        <title>Hono JSX on ApexKit</title>
        <script src="https://cdn.tailwindcss.com"></script>
      </head>
      <body class="bg-gray-950 text-white p-10 font-sans">
        <div class="max-w-2xl mx-auto bg-gray-900 border border-gray-800 p-8 rounded-2xl shadow-2xl">
          {props.children}
        </div>
      </body>
    </html>
  );
};

app.all("/", async (c) => {
  // Test Web Crypto Polyfills
  const uuid = crypto.randomUUID();
  const encoder = new TextEncoder();
  const data = encoder.encode("ApexKit is blazingly fast");
  
  // Test SubtleCrypto (Hashed in Rust!)
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  const hashHex = hashArray.map(b => b.toString(16).padStart(2, '0')).join('');

  // Fetch some DB data to render
  const pins = await $db.records.list("pins", { limit: 5 });

  return c.html(
    <Layout>
      <h1 class="text-3xl font-bold text-neon mb-2 text-[#ccff00]">Hono + JSX + WebCrypto</h1>
      <p class="text-gray-400 mb-6">Fully executed at the edge on ApexKit BaaS.</p>
      
      <div class="bg-black/50 p-4 rounded-lg mb-6 border border-gray-800 font-mono text-sm">
        <p><span class="text-gray-500">Generated UUID:</span> {uuid}</p>
        <p class="mt-2"><span class="text-gray-500">SHA-256 Hash via Polyfill:</span><br/>{hashHex}</p>
      </div>

      <h2 class="text-xl font-bold mb-3">Latest Pins:</h2>
      <ul class="space-y-2">
        {pins.items.map(p => (
          <li class="bg-gray-800 p-3 rounded border border-gray-700">{p.data.title}</li>
        ))}
      </ul>
    </Layout>
  );
});

export default async function (req: Request) {
  return app.fetch(req);
}




