export const getFileDocs = (colName: string) => {
  return {
    uploadAndLink: `const fileInput = document.getElementById('my-file');
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
const signed = await client.files.getSignedUrl(uploaded.filename, 3600);`,
  };
};
