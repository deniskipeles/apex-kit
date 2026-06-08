export const ScriptDocs = {
  runScript: `// Execute a script defined in Admin > Scripts
// (The script must be 'active' and trigger_type = 'manual' or 'public')
const result = await client.scripts.run('process-payment', {
    amount: 1500,
    currency: 'usd',
    item_id: 42
});

// The structure depends entirely on what your server-side script returns
console.log(result.success); 
console.log(result.receipt_url);`,
};
