import React, { useState } from 'react';
import { createPortal } from 'react-dom';
import {
  X,
  Copy,
  Check,
  Database,
  Search,
  BrainCircuit,
  Folder,
  Terminal,
  Server,
  Code2,
  Menu,
  AlertTriangle,
  ShieldAlert,
  Users,
  LayoutTemplate,
} from 'lucide-react';
import { Button, Badge } from '../../../components/ui/Elements';
import { Collection } from '../../../types';
import { APP_CONFIG } from '../../../config/app.config';

interface ApiDocsModalProps {
  isOpen: boolean;
  onClose: () => void;
  collection?: Collection;
  context?: 'collection' | 'users';
}

// --- SUB-COMPONENTS ---

const CodeBlock = ({
  label,
  code,
  className = '',
}: {
  label: string;
  code: string;
  className?: string;
}) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div
      className={`rounded-lg border border-border bg-[#0d1117] overflow-hidden my-4 group relative shadow-sm ${className}`}
    >
      <div className="flex items-center justify-between px-3 py-2 bg-[#161b22] border-b border-white/5">
        <span className="text-[10px] font-mono text-muted-foreground uppercase tracking-wider font-semibold">
          {label}
        </span>
        <button
          onClick={handleCopy}
          className="text-gray-400 hover:text-white transition-colors p-1 hover:bg-white/10 rounded"
          title="Copy"
        >
          {copied ? (
            <Check className="h-3.5 w-3.5 text-emerald-400" />
          ) : (
            <Copy className="h-3.5 w-3.5" />
          )}
        </button>
      </div>
      <div className="p-4 overflow-x-auto custom-scrollbar">
        <pre className="text-xs sm:text-sm font-mono text-[#e6edf3] leading-relaxed whitespace-pre">
          <code>{code.trim()}</code>
        </pre>
      </div>
    </div>
  );
};

const SectionHeader = ({
  title,
  icon: Icon,
  description,
}: {
  title: string;
  icon: any;
  description: string;
}) => (
  <div className="mb-6">
    <h4 className="text-xl font-bold flex items-center gap-2 text-foreground">
      <div className="p-2 rounded-md bg-primary/10 text-primary">
        <Icon className="h-5 w-5" />
      </div>
      {title}
    </h4>
    <p className="text-sm text-muted-foreground mt-1 ml-11">{description}</p>
  </div>
);

// --- MAIN COMPONENT ---

export const ApiDocsModal = ({ isOpen, onClose, collection, context }: ApiDocsModalProps) => {
  const [activeTab, setActiveTab] = useState(context === 'users' ? 'auth' : 'setup');
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  if (!isOpen) return null;

  const colName = collection?.name || 'posts';

  // Detect Environment for Setup Params
  const path = window.location.pathname;
  let initCode = `import { ApexKit } from '@apexkit/sdk';\n\nconst apex = new ApexKit('${APP_CONFIG.apiBaseUrl}');`;

  if (path.includes('/tenant/')) {
    const tenantId = path.split('/tenant/')[1].split('/')[0];
    initCode += `\n\n// Target Specific Tenant\nconst client = apex.tenant('${tenantId}');`;
  } else if (path.includes('/sandbox/')) {
    const sandboxId = path.split('/sandbox/')[1].split('/')[0];
    initCode += `\n\n// Target Sandbox Session\nconst client = apex.sandbox('${sandboxId}');`;
  } else {
    initCode += `\n\nconst client = apex;`;
  }

  const sections = [
    { id: 'setup', label: 'Initialization', icon: Server },
    { id: 'auth', label: 'Users & Auth', icon: Users },
    { id: 'records', label: 'Records (CRUD)', icon: Database },
    { id: 'search', label: 'Search Engine', icon: Search },
    { id: 'files', label: 'File Storage', icon: Folder },
    { id: 'ai', label: 'AI Actions', icon: BrainCircuit },
    { id: 'scripts', label: 'Scripts', icon: Terminal },
    { id: 'ssr', label: 'SSR & Templates', icon: LayoutTemplate },
    { id: 'errors', label: 'Error Handling', icon: ShieldAlert },
  ];

  return createPortal(
    <div className="fixed inset-0 z-[100] flex justify-center items-center isolate">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60 backdrop-blur-sm animate-in fade-in"
        onClick={onClose}
      />

      {/* Modal Container */}
      <div className="relative w-[95vw] md:w-[90vw] max-w-6xl h-[90vh] md:h-[85vh] bg-background border border-border shadow-2xl animate-in zoom-in-95 duration-200 flex flex-col md:flex-row rounded-xl overflow-hidden">
        {/* MOBILE HEADER */}
        <div className="md:hidden flex items-center justify-between p-4 border-b border-border bg-secondary/5">
          <button
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            className="p-2 hover:bg-secondary rounded-md"
          >
            <Menu className="h-5 w-5" />
          </button>
          <span className="font-bold text-sm flex items-center gap-2">
            <Code2 className="h-4 w-4 text-primary" /> SDK Guide
          </span>
          <button onClick={onClose} className="p-2 hover:bg-secondary rounded-md">
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* SIDEBAR NAVIGATION */}
        <div
          className={`
            absolute md:relative inset-y-0 left-0 z-20 w-64 bg-background/95 md:bg-secondary/5 border-r border-border flex flex-col shrink-0 transition-transform duration-300 transform
            ${isMobileMenuOpen ? 'translate-x-0' : '-translate-x-full'} md:translate-x-0 backdrop-blur-md md:backdrop-blur-none
        `}
        >
          <div className="hidden md:flex p-5 border-b border-border/50 items-center gap-3">
            <div className="h-8 w-8 rounded-lg bg-primary/10 flex items-center justify-center text-primary">
              <Code2 className="h-5 w-5" />
            </div>
            <div>
              <h2 className="font-bold text-sm">Developer Guide</h2>
              <p className="text-[10px] text-muted-foreground">ApexKit SDK v0.1</p>
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-3 space-y-1">
            {sections.map((s) => (
              <button
                key={s.id}
                onClick={() => {
                  setActiveTab(s.id);
                  setIsMobileMenuOpen(false);
                }}
                className={`w-full text-left px-3 py-2.5 rounded-md text-sm font-medium flex items-center gap-3 transition-colors ${activeTab === s.id ? 'bg-primary/10 text-primary border border-primary/20' : 'hover:bg-secondary text-muted-foreground hover:text-foreground border border-transparent'}`}
              >
                <s.icon
                  className={`h-4 w-4 ${activeTab === s.id ? 'text-primary' : 'opacity-70'}`}
                />
                {s.label}
              </button>
            ))}
          </div>
        </div>

        {/* MOBILE OVERLAY BACKDROP */}
        {isMobileMenuOpen && (
          <div
            className="absolute inset-0 bg-black/50 z-10 md:hidden"
            onClick={() => setIsMobileMenuOpen(false)}
          />
        )}

        {/* MAIN CONTENT */}
        <div className="flex-1 flex flex-col relative bg-background overflow-hidden">
          {/* Desktop Close Button */}
          <div className="absolute top-4 right-4 z-10 hidden md:block">
            <Button
              size="icon"
              variant="ghost"
              onClick={onClose}
              className="rounded-full h-8 w-8 hover:bg-secondary"
            >
              <X className="h-4 w-4" />
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-6 md:p-10 custom-scrollbar">
            <div className="max-w-3xl mx-auto">
              {/* SETUP */}
              {activeTab === 'setup' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300">
                  <SectionHeader
                    title="Initialization"
                    icon={Server}
                    description="Install the SDK and initialize the client. It automatically handles context switching for Tenants and Sandboxes."
                  />

                  <div className="space-y-6">
                    <CodeBlock label="Install via NPM" code="npm install @apexkit/sdk" />
                    <CodeBlock label="main.ts" code={initCode} />
                  </div>
                </div>
              )}

              {/* USERS & AUTH */}
              {activeTab === 'auth' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <SectionHeader
                    title="Users & Authentication"
                    icon={Users}
                    description="Complete workflows for user identity, roles, sessions, and recovery."
                  />

                  <div className="space-y-12">
                    {/* 1. Email / Password */}
                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]"></div>{' '}
                        1. Registration & Login
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        Standard email and password authentication. When logged in, the client SDK
                        automatically attaches the JWT token as a Bearer Header (
                        <code>Authorization: Bearer &lt;token&gt;</code>) to all future requests.
                      </p>
                      <CodeBlock
                        label="SDK (TypeScript / Javascript)"
                        code={`// 1. Register a new user
const res = await client.auth.register('user@example.com', 'password123');

// 2. Login (Token is automatically cached in the SDK instance)
const authData = await client.auth.login('user@example.com', 'password123');

console.log("JWT Token:", authData.token);
console.log("User Data:", authData.user);

// 3. Fetch current logged-in profile (Requires valid token)
const me = await client.auth.getMe();

// 4. Logout (Clears internal token)
client.auth.logout();`}
                      />
                    </div>

                    {/* 2. Password Reset Flow */}
                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-amber-500 shadow-[0_0_8px_rgba(245,158,11,0.5)]"></div>{' '}
                        2. Password Reset Flow
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        If a user forgets their password, they can request a reset link. This
                        requires SMTP to be configured in the dashboard. The email will contain a
                        secure token.
                      </p>
                      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                        <CodeBlock
                          label="Step 1: Request Reset (Frontend)"
                          code={`// Triggers an email to the user with a token
await fetch(client.baseUrl + '/api/v1/auth/request-password-reset', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ email: 'user@example.com' })
});`}
                        />
                        <CodeBlock
                          label="Step 2: Confirm Reset (Frontend)"
                          code={`// User clicked the link in their email and is now on your site
// e.g., https://yourfrontend.com/reset-password?token=abc-123

const urlParams = new URLSearchParams(window.location.search);
const token = urlParams.get('token');

await fetch(client.baseUrl + '/api/v1/auth/confirm-password-reset', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ 
    token: token, 
    new_password: 'new_secure_password' 
  })
});`}
                        />
                      </div>
                    </div>

                    {/* 3. Email Verification Flow */}
                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.5)]"></div>{' '}
                        3. Email Verification Flow
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        When a user registers, they are marked as unverified (
                        <code>is_verified = false</code>). You can restrict database access to only
                        verified users using custom Scripts, or manually prompt them.
                      </p>
                      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                        <CodeBlock
                          label="Resend Verification Email"
                          code={`// Triggers the verification email again
await fetch(client.baseUrl + '/api/v1/auth/verify/resend', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ email: 'user@example.com' })
});`}
                        />
                        <CodeBlock
                          label="Verify User (From Email Link)"
                          code={`// The email contains a link like: 
// https://your-app-url.com/api/v1/auth/verify?token=abc-123
// This is a GET request. When clicked, it automatically verifies the user in the database.

// You can also hit it via code:
await fetch(client.baseUrl + '/api/v1/auth/verify?token=abc-123');`}
                        />
                      </div>
                    </div>

                    {/* 4. OAuth */}
                    <div>
                      <h5 className="font-bold text-lg mb-2 flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-purple-500 shadow-[0_0_8px_rgba(168,85,247,0.5)]"></div>{' '}
                        4. OAuth (Google / GitHub)
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4 leading-relaxed">
                        Allow users to sign in using third-party providers. The SDK will redirect
                        the user to the provider, and ApexKit will handle the callback and token
                        generation.
                      </p>
                      <CodeBlock
                        label="SDK (TypeScript / Javascript)"
                        code={`// Redirects window.location to the OAuth consent screen.
// Once complete, ApexKit will redirect back to your specified callback URL.
// The resulting URL will have ?token=<jwt> appended to it.

client.auth.loginWithGoogle('https://myapp.com/auth-callback');

client.auth.loginWithGithub('https://myapp.com/auth-callback');

// --- On your frontend callback page ---
// const params = new URLSearchParams(window.location.search);
// const token = params.get('token');
// client.setToken(token);
`}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* RECORDS */}
              {activeTab === 'records' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <SectionHeader
                    title="Data Operations"
                    icon={Database}
                    description={`Standard CRUD operations for the '${colName}' collection.`}
                  />

                  <div className="grid grid-cols-1 gap-8">
                    <div>
                      <h5 className="font-bold text-sm mb-2 flex items-center gap-2">
                        <div className="w-1.5 h-1.5 rounded-full bg-blue-500"></div> List & Filter
                      </h5>
                      <p className="text-xs text-muted-foreground mb-3">
                        Fetch records with pagination, sorting, and MongoDB-style filtering.
                      </p>
                      <CodeBlock
                        label="JS"
                        code={`
// List with pagination, sorting, and relational expansion
const res = await client.collection('${colName}').list({
    page: 1,
    per_page: 20,
    sort: '-created', // descending
    filter: { 
        status: 'published',
        views: { $gt: 100 }
    },
    expand: 'author_id,comments' // Auto-fetches related records
});

console.log(res.items); // Array of records
console.log(res.total); // Total count
                                     `}
                      />
                    </div>

                    <div>
                      <h5 className="font-bold text-sm mb-2 flex items-center gap-2">
                        <div className="w-1.5 h-1.5 rounded-full bg-green-500"></div> Create,
                        Update, Delete
                      </h5>
                      <CodeBlock
                        label="JS"
                        code={`
// Get single record
const post = await client.collection('${colName}').get(123, { expand: 'author_id' });

// Create
const newRecord = await client.collection('${colName}').create({
    title: "New Item",
    active: true
});

// Update (Full replacement)
await client.collection('${colName}').update(newRecord.id, {
    title: "Updated Title"
});

// Delete
await client.collection('${colName}').delete(newRecord.id);
                                     `}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* SEARCH & VECTORS */}
              {activeTab === 'search' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-10">
                  <SectionHeader
                    title="Search Engine"
                    icon={Search}
                    description="High-performance full-text and semantic search capabilities."
                  />

                  <div className="space-y-8">
                    <div className="p-4 rounded-xl border border-blue-500/20 bg-blue-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-blue-500/10 text-blue-500 border-blue-500/20"
                          >
                            Fast
                          </Badge>
                          <h5 className="font-bold text-sm">Instant Search (Tantivy/OSE)</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Typo-tolerant, ultra-fast full text search. Returns snippets with HTML
                        highlights <code>&lt;b&gt;match&lt;/b&gt;</code>. Requires fields to have{' '}
                        <code>indexed: true</code>.
                      </p>
                      <CodeBlock
                        label="JS"
                        code={`
// "Harry Pottr" -> Matches "Harry Potter" (Typo tolerance)
const hits = await client.collection('${colName}').instantSearch("harry pottr", 10);

// hits[0].snippet => { title: "<b>Harry Potter</b> and the Stone..." }
// hits[0].score => 2.45
                                    `}
                      />
                    </div>

                    <div className="p-4 rounded-xl border border-purple-500/20 bg-purple-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-purple-500/10 text-purple-500 border-purple-500/20"
                          >
                            Semantic
                          </Badge>
                          <h5 className="font-bold text-sm">Text Vector Search</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Converts query text to embeddings to find conceptually similar records.
                        Requires text fields to have <code>vectorize: true</code>.
                      </p>
                      <CodeBlock
                        label="JS"
                        code={`
// Find records conceptually similar to the query
const results = await client.collection('${colName}').searchTextVector(
    "stories about space exploration", 
    5 // Limit
);

results.forEach(rec => {
    // Records are sorted by similarity automatically
    console.log(rec.id, rec._score);
});
                                    `}
                      />
                    </div>

                    <div className="p-4 rounded-xl border border-pink-500/20 bg-pink-500/5">
                      <div className="flex items-center justify-between mb-3">
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className="bg-pink-500/10 text-pink-500 border-pink-500/20"
                          >
                            Multimodal
                          </Badge>
                          <h5 className="font-bold text-sm">Image Vector Search</h5>
                        </div>
                      </div>
                      <p className="text-xs text-muted-foreground mb-4 leading-relaxed">
                        Find visually similar images by passing a Base64 image payload. Requires{' '}
                        <code>file</code> fields to have <code>vectorize: true</code>.
                      </p>
                      <CodeBlock
                        label="JS"
                        code={`
const base64Image = "data:image/png;base64,iVBORw0KGgo...";

const visualMatches = await client.collection('${colName}').searchImageVector(
    base64Image, 
    5 
);
                                    `}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* FILES */}
              {activeTab === 'files' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                  <SectionHeader
                    title="File Storage"
                    icon={Folder}
                    description="Upload and manage assets (Images, Documents)."
                  />

                  <div className="grid grid-cols-1 gap-6">
                    <div>
                      <h5 className="font-semibold text-sm mb-2">Upload & Link Workflow</h5>
                      <p className="text-xs text-muted-foreground mb-3">
                        Files are uploaded directly to the storage backend (Local/S3). The returned
                        file object contains the public URL.
                      </p>
                      <CodeBlock
                        label="JS"
                        code={`
const fileInput = document.getElementById('my-file');
const file = fileInput.files[0];

// 1. Upload
const uploaded = await client.files.upload(file);

console.log("File ID:", uploaded.id);
console.log("URL:", uploaded.url); 

// 2. Link to a record (store the generated filename)
await client.collection('${colName}').create({
    title: "Profile",
    avatar: uploaded.filename 
});

// 3. Generate Secure Signed URLs (For private S3 buckets)
const signed = await client.files.getSignedUrl(uploaded.filename, 3600);
                                     `}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* AI ACTIONS */}
              {activeTab === 'ai' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                  <SectionHeader
                    title="AI Actions"
                    icon={BrainCircuit}
                    description="Execute server-side LLM prompts securely without exposing API keys."
                  />

                  <CodeBlock
                    label="JS"
                    code={`
// Run a pre-defined prompt template (configured in Admin > AI Actions)
// e.g. Slug: 'summarize-content'
const response = await client.ai.run('summarize-content', {
    text: "Long article content here...",
    tone: "professional"
});

console.log(response.result); // The raw AI output text
console.log(response.metadata); // Grounding/Citation data (if Google Search is enabled)
                             `}
                  />
                </div>
              )}

              {/* SCRIPTS */}
              {activeTab === 'scripts' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                  <SectionHeader
                    title="Server Scripts"
                    icon={Terminal}
                    description="Trigger custom backend JavaScript logic via API endpoints."
                  />

                  <CodeBlock
                    label="JS"
                    code={`
// Execute a script defined in Admin > Scripts
// (The script must be 'active' and trigger_type = 'manual' or 'public')
const result = await client.scripts.run('process-payment', {
    amount: 1500,
    currency: 'usd',
    item_id: 42
});

// The structure depends entirely on what your server-side script returns
console.log(result.success); 
console.log(result.receipt_url); 
                             `}
                  />
                </div>
              )}

              {/* SSR & TEMPLATES */}
              {activeTab === 'ssr' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-6">
                  <SectionHeader
                    title="Server-Side Rendering (SSR)"
                    icon={LayoutTemplate}
                    description="Build dynamic pages using JavaScript Frontmatter and HTML."
                  />

                  <p className="text-sm text-muted-foreground mb-4">
                    ApexKit templates use an Astro-like syntax. You can write Server-Side JavaScript
                    inside the <code>---</code> block at the top of the file to fetch database
                    records, and the returned JSON becomes available to the HTML below it.
                  </p>

                  <CodeBlock
                    label="HTML"
                    code={`---
export default async function(req) {
    const payload = await req.json();
    
    // You have access to the exact same $db API here!
    const post = await $db.records.get('posts', payload.params.id);
    
    return { 
        post: post,
        user_agent: payload.headers['user-agent'] 
    };
}
---

<div class="max-w-2xl mx-auto p-4">
    <h1>{{ post.data.title }}</h1>
    <p>{{ post.data.content }}</p>
    
    <small>Rendered for: {{ user_agent }}</small>
</div>
                             `}
                  />
                  <h3>OR</h3>
                  <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-12">
                    <SectionHeader
                      title="Server-Side Rendering (SSR)"
                      icon={LayoutTemplate}
                      description="Build fast, secure, and dynamic pages using JavaScript Controllers, Tera HTML, and HTMX."
                    />

                    {/* Part 1: Anatomy of a Template */}
                    <div>
                      <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                        1. Anatomy of a Template
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4">
                        A template consists of two parts: a Server-Side JavaScript Controller
                        (wrapped in a special script tag) and Tera HTML. The JS executes securely on
                        the backend, fetches data, and passes JSON to the HTML engine.
                      </p>
                      <CodeBlock
                        label="HTML / TERA"
                        code={`<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    
    // Fetch data using the backend $db API
    const posts = await $db.records.list('posts', { limit: 5 });
    
    // Return variables to the HTML
    return { 
        posts: posts.items,
        title: "Latest News" 
    };
}
// ---@@ssr
</script>

<!-- The HTML receives the returned JSON -->
<div class="container">
    <h1>{{ title }}</h1>
    <ul>
        {% for post in posts %}
            <li>{{ post.data.title }}</li>
        {% endfor %}
    </ul>
</div>`}
                      />
                    </div>

                    {/* Part 2: The Request & Auth */}
                    <div>
                      <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                        2. Routing, Request, and Auth
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4">
                        Templates are automatically accessible at{' '}
                        <code>/render/&#123;slug&#125;</code>. The <code>req.json()</code> object
                        contains URL parameters, headers, and the authenticated user's details.
                      </p>
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                        <CodeBlock
                          label="Request Payload"
                          code={`{
  "params": { 
    // From URL /render/posts?id=5
    "id": "5" 
  },
  "headers": { 
    "user-agent": "Mozilla/5.0..." 
  },
  "is_htmx": true,
  "auth": { 
    // Null if not logged in
    "id": 1, 
    "email": "user@test.com", 
    "role": "admin" 
  }
}`}
                        />
                        <CodeBlock
                          label="Protecting a Route"
                          code={`// Inside your ---@@ssr block
export default async function(req) {
    const payload = await req.json();
    
    // Block unauthenticated users
    if (!payload.auth) {
        return new Response(
            { error: "Unauthorized" }, 
            { status: 401 }
        );
    }
    
    return { user: payload.auth };
}`}
                        />
                      </div>
                    </div>

                    {/* Part 3: Client-Side Universal Script */}
                    <div>
                      <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                        3. Client-Side: <code className="text-primary">apex.js</code> and HTMX
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4">
                        To make your frontend work flawlessly across Root, Tenants, and Sandboxes,
                        include the built-in <code>apex.js</code> script. It automatically manages
                        JWT tokens, scope routing, and intercepts HTMX and <code>fetch()</code>{' '}
                        requests to append the correct headers.
                      </p>
                      <CodeBlock
                        label="base-layout template"
                        code={`<!DOCTYPE html>
<html>
<head>
    <script src="/static/js/htmx.js"></script>
    <script src="/static/js/alpine.js" defer></script>
    <!-- Automatically handles Auth Headers & Scope Routing! -->
    <script src="/static/js/apex.js"></script>
</head>
<body>
    <!-- Login Example -->
    <form onsubmit="event.preventDefault(); $apex.login(email.value, password.value).then(res => { if(res.ok) window.location.href = $apex.scope + '/render/dashboard'; })">
        <input id="email" type="email">
        <input id="password" type="password">
        <button type="submit">Login</button>
    </form>

    <!-- HTMX automatically gets the Token and Scope Prefix! -->
    <button hx-post="/api/v1/run/buy_now">Purchase</button>
    
    <button onclick="$apex.logout()">Logout</button>
</body>
</html>`}
                      />
                    </div>

                    {/* Part 4: Components */}
                    <div>
                      <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                        4. Components & Includes
                      </h5>
                      <p className="text-sm text-muted-foreground mb-4">
                        You can break your UI into reusable components. Note: The SSR JavaScript{' '}
                        <b>only executes on the main requested template</b> (the Controller). The
                        main template must fetch the data and pass it down, and the components just
                        render the HTML.
                      </p>
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                        <CodeBlock
                          label="components/navbar template"
                          code={`<!-- No JS allowed here. Just HTML/Tera -->
<nav>
    <div class="logo">My App</div>
    {% if user %}
        <span>Hello, {{ user.email }}</span>
        <button onclick="$apex.logout()">Logout</button>
    {% else %}
        <a href="/render/login">Login</a>
    {% endif %}
</nav>`}
                        />
                        <CodeBlock
                          label="dashboard template"
                          code={`<script>
// ---@@ssr
export default async function(req) {
    const payload = await req.json();
    return { user: payload.auth };
}
// ---@@ssr
</script>

<div>
    <!-- Include the component -->
    {% include "components/navbar" %}

    <main>Dashboard Content</main>
</div>`}
                        />
                      </div>
                    </div>

                    {/* Part 5: Tera Syntax Cheat Sheet */}
                    <div>
                      <h5 className="font-bold text-base mb-2 text-foreground border-b border-border pb-2">
                        5. Tera Syntax Cheat Sheet
                      </h5>
                      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                        <div className="bg-secondary/10 p-4 rounded-md border border-border text-sm font-mono text-muted-foreground">
                          <span className="text-primary font-bold block mb-1">
                            Variables & Filters
                          </span>
                          &#123;&#123; user.email &#125;&#125;
                          <br />
                          &#123;&#123; post.data.title | upper &#125;&#125;
                          <br />
                          &#123;&#123; post.data.content | safe &#125;&#125; (Renders HTML)
                          <br />
                          &#123;&#123; data | debug &#125;&#125; (Dumps JSON)
                        </div>
                        <div className="bg-secondary/10 p-4 rounded-md border border-border text-sm font-mono text-muted-foreground">
                          <span className="text-primary font-bold block mb-1">Logic & Loops</span>
                          &#123;% if user.role == "admin" %&#125; ... &#123;% endif %&#125;
                          <br />
                          <br />
                          &#123;% for item in items %&#125;
                          <br />
                          &nbsp;&nbsp;&lt;li&gt;&#123;&#123; loop.index &#125;&#125;. &#123;&#123;
                          item.name &#125;&#125;&lt;/li&gt;
                          <br />
                          &#123;% else %&#125; No items.
                          <br />
                          &#123;% endfor %&#125;
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* ERRORS */}
              {activeTab === 'errors' && (
                <div className="animate-in fade-in slide-in-from-bottom-2 duration-300 space-y-8">
                  <SectionHeader
                    title="Error Handling"
                    icon={AlertTriangle}
                    description="Standardized JSON error responses across all APIs."
                  />

                  <div className="grid gap-6">
                    <div>
                      <h5 className="font-bold text-sm mb-3">Response Format</h5>
                      <CodeBlock
                        label="JSON"
                        code={`
{
  "error": "not_found",           // Short code
  "message": "Record not found",  // Human readable message
  "status": 404,                  // HTTP Status Code
  "details": { ... }              // Optional validation arrays
}
                                    `}
                      />
                    </div>

                    <div>
                      <h5 className="font-bold text-sm mb-3">Common Status Codes</h5>
                      <div className="rounded-lg border border-border overflow-hidden">
                        <table className="w-full text-sm text-left">
                          <thead className="bg-secondary/10 text-muted-foreground text-xs uppercase">
                            <tr>
                              <th className="px-4 py-3 font-medium">Code</th>
                              <th className="px-4 py-3 font-medium">Error</th>
                              <th className="px-4 py-3 font-medium">Description</th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-border">
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">400</td>
                              <td className="px-4 py-3">input_validation</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Invalid input or bad JSON syntax.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">401</td>
                              <td className="px-4 py-3">unauthorized</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Missing or invalid Auth Token.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">403</td>
                              <td className="px-4 py-3">forbidden</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Authenticated, but policy denied access.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">404</td>
                              <td className="px-4 py-3">not_found</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Resource does not exist.
                              </td>
                            </tr>
                            <tr>
                              <td className="px-4 py-3 font-mono text-xs">422</td>
                              <td className="px-4 py-3">validation_error</td>
                              <td className="px-4 py-3 text-muted-foreground">
                                Schema constraint violation (e.g. required field missing).
                              </td>
                            </tr>
                          </tbody>
                        </table>
                      </div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
};
