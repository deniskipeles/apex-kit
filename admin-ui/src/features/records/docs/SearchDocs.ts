export const getSearchDocs = (colName: string) => {
  return {
    instantSearch: `// "Harry Pottr" -> Matches "Harry Potter" (Typo tolerance)
const hits = await client.collection('${colName}').instantSearch("harry pottr", 10);

// hits[0].snippet => { title: "<b>Harry Potter</b> and the Stone..." }
// hits[0].score => 2.45`,

    textVectorSearch: `// Find records conceptually similar to the query
const results = await client.collection('${colName}').searchTextVector(
    "stories about space exploration", 
    5 // Limit
);

results.forEach(rec => {
    // Records are sorted by similarity automatically
    console.log(rec.id, rec._score);
});`,

    imageVectorSearch: `const base64Image = "data:image/png;base64,iVBORw0KGgo...";

const visualMatches = await client.collection('${colName}').searchImageVector(
    base64Image, 
    5 
);`,
  };
};
