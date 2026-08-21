Because ApexKit provides a preopened, isolated Virtual File System (`.`) and WASI Preview-1 execution engine, you can run standalone `wasm32-wasi` tools as micro-services directly inside your JavaScript webhooks using `$wasm.runWasi`.

Some of the most useful single-threaded WASI command-line tools for file processing in **ApexKit Drive** include:

---

### 1. Document & PDF Processing

#### **A. `pdftotext.wasm` / `pdfimages.wasm` (Poppler)**
* **Use Case:** Full-text indexing and thumbnail generation for uploaded PDFs.
* **What it does:** Extracts plain text from PDFs for search indexing or dumps embedded images without needing external Python/Node services.
* **Webhook Example:**
  ```javascript
  // Extract all text from an uploaded PDF for Tantivy/OSE search indexing
  await $wasm.runWasi("pdftotext.wasm", [
    "-layout",
    `${tmpDir}/input.pdf`,
    `${tmpDir}/extracted.txt`
  ]);
  const textContent = await $fs.read(`${tmpDir}/extracted.txt`);
  ```

#### **B. `mutool.wasm` (MuPDF)**
* **Use Case:** Rendering high-fidelity PDF page previews to PNG/JPEG.
* **What it does:** Renders page 1 of any PDF document into a sharp thumbnail for the Drive UI.
* **Webhook Example:**
  ```javascript
  // Render page 1 of a PDF to a PNG preview
  await $wasm.runWasi("mutool.wasm", [
    "draw",
    "-o", `${tmpDir}/preview.png`,
    "-w", "800",
    "-h", "1000",
    `${tmpDir}/input.pdf`,
    "1" // Page 1
  ]);
  ```

#### **C. `pandoc.wasm`**
* **Use Case:** Universal Document Converter.
* **What it does:** Converts between Markdown, HTML, DOCX, EPUB, LaTeX, and ODT.
* **Webhook Example:**
  ```javascript
  // Convert an uploaded Markdown document into a Word Document (.docx)
  await $wasm.runWasi("pandoc.wasm", [
    "-f", "markdown",
    "-t", "docx",
    "-o", `${tmpDir}/output.docx`,
    `${tmpDir}/document.md`
  ]);
  ```

#### **D. `typst.wasm`**
* **Use Case:** Ultra-fast dynamic PDF generation and report compilation from templates.
* **What it does:** Compiles Typst markup files directly into publication-quality PDFs or SVG pages in milliseconds.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("typst.wasm", [
    "compile",
    `${tmpDir}/invoice.typ`,
    `${tmpDir}/invoice.pdf`
  ]);
  ```

---

### 2. Image Optimization & Format Conversion

#### **A. `oxipng.wasm`**
* **Use Case:** Lossless PNG compressor.
* **What it does:** Optimizes PNG files uploaded by users, reducing file sizes by 15–40% before storing them permanently in your S3 or local bucket.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("oxipng.wasm", [
    "-o", "4",
    "--strip", "all",
    `${tmpDir}/input.png`,
    "--out", `${tmpDir}/optimized.png`
  ]);
  ```

#### **B. `cwebp.wasm` / `dwebp.wasm` (libwebp)**
* **Use Case:** WebP encoder/decoder.
* **What it does:** Converts heavy JPG/PNG images into modern WebP format on upload.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("cwebp.wasm", [
    "-q", "80",
    `${tmpDir}/input.jpg`,
    "-o", `${tmpDir}/output.webp`
  ]);
  ```

#### **C. `gifsicle.wasm`**
* **Use Case:** Animated GIF optimizer and resizer.
* **What it does:** Shrinks large animated GIFs, extracts frames, or scales dimensions without high CPU overhead.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("gifsicle.wasm", [
    "-O3",
    "--resize-fit", "480x480",
    `${tmpDir}/animation.gif`,
    "-o", `${tmpDir}/optimized.gif`
  ]);
  ```

---

### 3. Archive & Compression Engines (Beyond ZIP)

*ApexKit has built-in `$zip`, but for other archive formats:*

#### **A. `7z.wasm` / `p7zip.wasm`**
* **Use Case:** Unpacking RAR, 7z, ISO, TAR.XZ, and CAB archives.
* **What it does:** Allows users to preview or extract complex archives directly inside Apex Drive.
* **Webhook Example:**
  ```javascript
  // Extract a 7z or RAR archive into a temporary folder
  await $wasm.runWasi("7z.wasm", [
    "x",
    `${tmpDir}/archive.7z`,
    `-o${tmpDir}/extracted/`
  ]);
  ```

#### **B. `zstd.wasm`**
* **Use Case:** High-speed, high-ratio data compression.
* **What it does:** Compress log files, raw dataset dumps, or system backups with Zstandard.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("zstd.wasm", [
    "-19", // Ultra compression
    `${tmpDir}/database.dump`,
    "-o", `${tmpDir}/database.dump.zst`
  ]);
  ```

---

### 4. Audio Processing & Deep Metadata

#### **A. `ffprobe.wasm`**
* **Use Case:** Exact audio/video stream inspection.
* **What it does:** Extracts detailed technical metadata (resolution, exact duration, frame rate, audio channels, color profile) as structured JSON.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("ffprobe.wasm", [
    "-v", "quiet",
    "-print_format", "json",
    "-show_format",
    "-show_streams",
    `${tmpDir}/video.mp4`
  ]);
  ```

#### **B. `sox.wasm` (Sound eXchange)**
* **Use Case:** Audio normalization, trimming, and effects.
* **What it does:** Removes silence, normalizes volume peaks, applies high/low-pass filters, or trims audio tracks without video overhead.
* **Webhook Example:**
  ```javascript
  // Normalize volume and trim the first 30 seconds for an audio preview
  await $wasm.runWasi("sox.wasm", [
    `${tmpDir}/voice_note.wav`,
    `${tmpDir}/preview.wav`,
    "trim", "0", "30",
    "norm", "-1"
  ]);
  ```

---

### 5. Data & Structured Text Utilities

#### **A. `jq.wasm`**
* **Use Case:** Server-side transformation of massive JSON files.
* **What it does:** Filters, transforms, or extracts sub-fields from 50MB+ JSON datasets without loading the entire JSON object into the QuickJS memory heap.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("jq.wasm", [
    ".[0:100] | .[] | { id: .user_id, name: .name }",
    `${tmpDir}/huge_dataset.json`
  ]);
  ```

#### **B. `tesseract.wasm`**
* **Use Case:** Offline Optical Character Recognition (OCR).
* **What it does:** Extracts text from receipts, invoices, screenshots, and scanned documents, saving the result into `drive_items.metadata` so scanned images become searchable in your search bar.
* **Webhook Example:**
  ```javascript
  await $wasm.runWasi("tesseract.wasm", [
    `${tmpDir}/scanned_receipt.png`,
    `${tmpDir}/ocr_output`,
    "-l", "eng"
  ]);
  const extractedText = await $fs.read(`${tmpDir}/ocr_output.txt`);
  ```

---

### Standard Workflow Pattern in Webhooks

To use any of these tools in your ApexKit scripts, follow this standard pattern:

```javascript
// 1. Prepare isolated workspace directory
const tmpDir = `processing/${fileId}`;
await $fs.mkdir(tmpDir);

// 2. Read physical file from Apex Storage and write to VFS scratchpad
const rawData = await $files.read(fileRecord.physical_file);
await $fs.writeBytes(`${tmpDir}/input.pdf`, rawData);

// 3. Execute the single-threaded WASI CLI tool
await $wasm.runWasi("mutool.wasm", [
  "draw",
  "-o", `${tmpDir}/thumb.png`,
  `${tmpDir}/input.pdf`,
  "1"
], { memoryMb: 256, timeoutMs: 30000 });

// 4. Read back result and save thumbnail to persistent storage
if (await $fs.exists(`${tmpDir}/thumb.png`)) {
  const thumbBase64 = await $fs.readBytes(`${tmpDir}/thumb.png`);
  const savedFile = await $files.save("thumbnail.png", thumbBase64, "image/png");
  
  // 5. Update record with generated preview URL
  await $db.records.update("drive_items", fileId, {
    metadata: { ...fileRecord.metadata, thumbnail: savedFile.url }
  });
}

// 6. Clean up temporary files
await $fs.delete(tmpDir).catch(() => {});
```