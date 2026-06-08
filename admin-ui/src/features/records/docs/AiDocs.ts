export const getAiDocs = (colName: string) => {
  return {
    runAction: `// Run a pre-defined prompt template (configured in Admin > AI Actions)
// e.g. Slug: 'summarize-content'
const response = await client.ai.run('summarize-content', {
    text: "Long article content here...",
    tone: "professional"
});

console.log(response.result); // The raw AI output text
console.log(response.metadata); // Grounding/Citation data (if Google Search is enabled)`,
  };
};
