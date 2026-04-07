# AI & Scripts

The `apex.ai` and `apex.scripts` namespaces provide access to Generative AI features and custom server-side scripts.

---

## AI Actions (LLMs)

Run predefined Generative AI prompt templates securely using the `ai` namespace.

### Methods

- `ai.getActions()`
- `ai.createAction(data)`
- `ai.deleteAction(id)`
- `ai.run(slug, variables)`
- `ai.exportActions()`
- `ai.importActions(file)`

### Running an AI Action

```typescript
const response = await apex.ai.run('content-summarizer', {
    text: "Long article body...",
    length: "short"
});

console.log(response.result); // AI-generated text
console.log(response.metadata); // Citations, search sources, etc.
```

---

## Architect (AI Sessions)

The Architect namespace allows for collaborative code and schema generation using AI.

### Methods

- `ai.listSessions()`
- `ai.createSession(name, initialPrompt?, model?, cloneStrategy?, cloneRecordLimit?)`
- `ai.deleteSession(id)`
- `ai.chat(sessionId, prompt, model)`
- `ai.applySessionChanges(sessionId)`
- `ai.publishSession(sessionId)`

### Managing Sessions

```typescript
// Create a session for code editing
const session = await apex.ai.createSession('New Plugin Dev', 'Create a blog system');

// Chat with the AI session
const chatRes = await apex.ai.chat(session.id, 'Add a comments field to the post schema', 'gpt-4');

// Apply generated changes
await apex.ai.applySessionChanges(session.id);
```

### AI Utils

- `ai.listPlugins()`
- `ai.editCode(prompt, currentCode, contextType, model)`

---

## Scripts & Templates

Execute custom server-side logic and manage content templates.

### Scripts Namespace

- `scripts.list()`
- `scripts.create(data)`
- `scripts.delete(id)`
- `scripts.run(name, variables)`
- `scripts.export()`
- `scripts.import(file)`

```typescript
// Run a server-side script
const result = await apex.scripts.run('process-payment', { amount: 100 });
```

### Templates Namespace

- `templates.list()`
- `templates.create(data)`
- `templates.update(id, data)`
- `templates.delete(id)`
- `templates.export()`
- `templates.import(file)`

```typescript
// Create a new content template
const template = await apex.templates.create({
    slug: 'email-welcome',
    content: '<h1>Welcome, {{name}}!</h1>'
});
```
