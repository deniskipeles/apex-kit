export const getCrudDocs = (colName: string) => {
  return {
    listAndFilter: `// List with pagination, sorting, and relational expansion
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
console.log(res.total); // Total count`,

    createUpdateDelete: `// Get single record
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
await client.collection('${colName}').delete(newRecord.id);`,
  };
};
