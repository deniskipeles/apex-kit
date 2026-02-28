# 🎥 System Media Processor

**Service ID:** `system-media-processor`  
**Access:** Public (All Tenants)  
**Output Location:** `storage/tenants/{your_id}/tmp/{output_name}/`

This service allows you to transcode videos, generate HLS streams, convert formats, and optimize media using the server's native GPU/CPU resources via FFmpeg.

---

Here is a production-grade, multi-tenant-ready FFmpeg processing script. This script is designed to be deployed in the **Root App** with **Public Visibility**, allowing all tenants to invoke it as a "Video Processing Service".

### Capabilities
1.  **HLS (m3u8) Transcoding**: Generates adaptive bitrate streams (1080p, 720p, 480p) with configurable segment duration.
2.  **Generic Conversion**: Handles simple MP4/WebM conversions if HLS is not requested.
3.  **Smart Storage**:
    *   Saves output to the tenant's scoped storage automatically.
    *   (Optional) Uses `rclone` to push to a custom S3 bucket if credentials are provided in the payload.
4.  **Concurrency Control**: Uses `$cmd.spawn` for non-blocking execution, respecting the system limits.
5.  **Progress Tracking**: Returns a `job_id` so the tenant can poll status via `$cmd.status(pid)` (if exposed) or via a callback webhook (best practice).

### Root Script: `system-media-processor`

**Configuration:**
*   **Name**: `system-media-processor`
*   **Trigger**: `manual`
*   **Visibility**: `public`
*   **Code**:

```javascript
/**
 * ApexKit Ultimate Media Processor v7.5 (Production)
 * Fixed critical race condition in script generation sequence.
 * 
 * Capabilities:
 * - HLS/DASH Adaptive Bitrate (ABR) Streaming
 * - Codec Support: H.264, HEVC, AV1 (SVT-AV1), VP9
 * - Optimized GIF & Thumbnail Generation
 * - Smart Storage: Local Tenant Tmp OR Cloud (S3/R2/Rclone)
 * - Real-time Progress Events via WebSocket
 * 
 * @param {string} input_file - Filename in tenant uploads OR http(s) URL
 * @param {string} output_name - Desired name for output folder/file (no ext)
 * @param {string} [format="mp4"] - "mp4", "webm", "gif", "jpg", "png", "hls", "dash", "both"
 * @param {string} [codec="h264"] - "h264", "hevc", "av1", "vp9"
 * @param {boolean} [gpu=false] - Use GPU acceleration (NVENC)
 * @param {string} [ladder="auto"] - "auto" or "min-max" (e.g., "144-480")
 * @param {number} [segment_time=10] - Segment duration in seconds
 * @param {object} [rclone] - Optional R2/S3 config
 * @param {boolean} [force_reencode=false] - Force reencode even if same codec
 */
export default async function (req) {
    const body = await req.json();

    const {
        input_file,
        output_name,
        format = "mp4",
        codec = null,
        gpu = false,
        ladder = "auto",
        segment_time = 10,
        rclone = null,
        force_reencode = false
    } = body;

    // 1. Resolve Tenant Scope
    let tenantId = "root_test";
    if (body.__caller_scope && body.__caller_scope.Tenant) {
        tenantId = body.__caller_scope.Tenant;
    } else if (body.tenant_id) {
        tenantId = body.tenant_id;
    }

    if (!input_file || !output_name)
        return new Response({ success: false, error: "Missing input_file or output_name" }, { status: 400 });

    const jobId = $util.uuid().split("-")[0];
    const jobChannel = `job_${jobId}`;

    // Paths
    const fsWorkDir = `${jobChannel}`;
    const sysWorkDir = `storage/system/tmp/${jobChannel}`;

    await $fs.mkdir(fsWorkDir);

    const inputPath = input_file.startsWith("http")
        ? input_file
        : `storage/tenants/${tenantId}/uploads/${input_file}`;

    // Determine Destination Directory
    // HLS/DASH go into a subfolder named after output_name. Single files go to root of tmp.
    let destSubPath = (format === 'hls' || format === 'dash' || format === 'both') ? output_name : '';
    let sysDestDir = `storage/tenants/${tenantId}/tmp/${destSubPath}`;

    if (tenantId === "root_test") {
        sysDestDir = `storage/tmp/root_test/${destSubPath}`;
    }

    let args = ["-y", "-i", inputPath];

    // =====================================================
    // 🖼️ 1️⃣ SINGLE FILE CONVERSION (MP4, MKV, GIF, JPG)
    // =====================================================
    const singleFileFormats = ["mp4", "mkv", "webm", "mov", "avi", "flv", "gif", "jpg", "png"];

    if (singleFileFormats.includes(format)) {
        let outputFile = `${sysWorkDir}/${output_name}.${format}`;

        if (format === "gif") {
            args.push(
                "-t", "5",
                "-vf", "fps=10,scale=320:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                "-loop", "0",
                outputFile
            );
        } else if (format === "jpg" || format === "png") {
            args.push("-ss", "00:00:05", "-vframes", "1", outputFile);
        } else {
            if (!force_reencode && !codec) {
                args.push("-c", "copy", outputFile);
            } else {
                const videoCodec = getCodec(codec || "h264", gpu);
                args.push(...videoCodec, "-c:a", "aac");
                args.push(outputFile);
            }
        }

        const shellEscape = (arg) => "'" + arg.replace(/'/g, "'\\''") + "'";
        const escapedArgs = args.map(shellEscape);

        // [FIX] Pass resolved sysDestDir and jobChannel
        const postProcessCmds = await generateStorageCommands(rclone, jobChannel, sysWorkDir, sysDestDir, output_name, format);

        const fullWrapperScript = `
            #!/bin/bash
            echo "Starting FFmpeg..."
            ffmpeg ${escapedArgs.join(" ")} > ${sysWorkDir}/ffmpeg.log 2>&1
            FF_CODE=$?
            if [ $FF_CODE -eq 0 ]; then
                echo "FFmpeg Success"
                ${postProcessCmds}
            else
                echo "FFmpeg Failed"
                cat ${sysWorkDir}/ffmpeg.log
            fi
            rm -rf ${sysWorkDir}
        `;

        const wrapperPath = `${fsWorkDir}/proc.sh`;
        await $fs.write(wrapperPath, fullWrapperScript);
        await $cmd.run("chmod", ["+x", `storage/system/tmp/${wrapperPath}`]);

        const job = await $cmd.spawn("bash", [`storage/system/tmp/${wrapperPath}`], {
            timeout: 3600000,
            onProgress: {
                regex: ".*(time=[0-9:.]+|frame=.*fps=.*size=.*).*",
                channel: jobChannel,
                event: "progress"
            }
        });

        return new Response({
            success: true,
            job_id: job.pid,
            channel: jobChannel,
            output: `${output_name}.${format}`
        });
    }

    // =====================================================
    // 🎬 2️⃣ STREAMING (HLS/DASH)
    // =====================================================

    const probe = await $cmd.run("ffprobe", [
        "-v", "error",
        "-select_streams", "v:0",
        "-show_entries", "stream=height",
        "-of", "json",
        inputPath
    ], { timeout: 10000 });

    if (probe.status !== 0) {
        return new Response({ error: "Probe failed", details: probe.stderr }, { status: 500 });
    }

    const inputHeight = JSON.parse(probe.stdout).streams[0].height;
    const ladderProfiles = buildAutoLadder(inputHeight, ladder);

    // Check if requested codec is available, fallback to h264 if not
    let finalCodec = codec || "h264";
    const codecCheck = await $cmd.run("ffmpeg", ["-encoders"], { timeout: 5000 });
    if (codecCheck.status === 0) {
        const encoders = codecCheck.stdout;
        if (finalCodec === "av1" && !encoders.includes("libsvtav1")) {
            console.log("AV1 encoder not available, falling back to h264");
            finalCodec = "h264";
        }
        if (finalCodec === "hevc" && !encoders.includes("libx265")) {
            console.log("HEVC encoder not available, falling back to h264");
            finalCodec = "h264";
        }
    }

    const videoCodec = getCodec(finalCodec, gpu);

    // Build filter complex
    let filter = "";
    ladderProfiles.forEach((l, i) => {
        filter += `[0:v]scale=-2:${l.h}[v${i}];`;
    });

    // Split audio for each variant
    filter += `[0:a]asplit=${ladderProfiles.length}` + ladderProfiles.map((_, i) => `[a${i}]`).join("") + ";";

    args.push("-filter_complex", filter.slice(0, -1));

    // Map video and audio for each variant
    ladderProfiles.forEach((l, i) => {
        args.push(
            "-map", `[v${i}]`,
            "-map", `[a${i}]`,
            ...videoCodec,
            "-c:a", "aac",
            `-b:v:${i}`, l.b,
            `-b:a:${i}`, "128k"
        );
    });

    const segmentType = (finalCodec === "h264") ? "mpegts" : "fmp4";
    const segmentExt = (segmentType === "mpegts") ? "ts" : "m4s";

    if (format === "hls" || format === "both") {
        const varStreamMap = ladderProfiles.map((_, i) => `v:${i},a:${i}`).join(" ");

        args.push(
            "-f", "hls",
            "-hls_time", segment_time.toString(),
            "-hls_segment_type", segmentType,
            "-hls_segment_filename", `${sysWorkDir}/hls/v%v/seg_%03d.${segmentExt}`,
            "-master_pl_name", "master.m3u8",
            "-var_stream_map", varStreamMap,
            "-hls_playlist_type", "vod",
            "-hls_flags", "independent_segments",
            `${sysWorkDir}/hls/v%v/playlist.m3u8`
        );
    }

    if (format === "dash" || format === "both") {
        // DASH: Streams are interleaved: 0=v0, 1=a0, 2=v1, 3=a1, 4=v2, 5=a2, 6=v3, 7=a3
        const videoStreams = ladderProfiles.map((_, i) => i * 2).join(",");
        const audioStreams = ladderProfiles.map((_, i) => (i * 2) + 1).join(",");

        args.push(
            "-f", "dash",
            "-seg_duration", segment_time.toString(),
            "-streaming", "0",
            "-use_template", "1",
            "-use_timeline", "1",
            "-adaptation_sets", `id=0,streams=${videoStreams} id=1,streams=${audioStreams}`,
            "-init_seg_name", "init_$RepresentationID$.mp4",
            "-media_seg_name", "chunk_$RepresentationID$_$Number$.m4s",
            `${sysWorkDir}/dash/manifest.mpd`
        );
    }

    const shellEscape = (arg) => "'" + arg.replace(/'/g, "'\\''") + "'";
    const escapedArgs = args.map(shellEscape);

    const postProcessCmds = await generateStorageCommands(rclone, jobChannel, sysWorkDir, sysDestDir, output_name, format);

    const wrapperScript = `
        #!/bin/bash
        set -e
        mkdir -p ${sysWorkDir}/hls/v0 ${sysWorkDir}/hls/v1 ${sysWorkDir}/hls/v2 ${sysWorkDir}/hls/v3 ${sysWorkDir}/dash
        echo "Starting Transcode at $(date)" >> ${sysWorkDir}/ffmpeg.log
        echo "Codec: ${finalCodec}, Format: ${format}" >> ${sysWorkDir}/ffmpeg.log
        echo "Command: ffmpeg ${escapedArgs.join(" ")}" >> ${sysWorkDir}/ffmpeg.log
        ffmpeg ${escapedArgs.join(" ")} >> ${sysWorkDir}/ffmpeg.log 2>&1 || {
            FF_CODE=$?
            echo "FFmpeg FAILED with code $FF_CODE at $(date)" >> ${sysWorkDir}/ffmpeg.log
            echo "=== LAST 100 LINES OF LOG ===" 
            tail -n 100 ${sysWorkDir}/ffmpeg.log
            exit $FF_CODE
        }
        echo "FFmpeg SUCCESS at $(date)" >> ${sysWorkDir}/ffmpeg.log
        echo "=== Generated files ===" >> ${sysWorkDir}/ffmpeg.log
        find ${sysWorkDir} -type f >> ${sysWorkDir}/ffmpeg.log 2>&1 || true
        ${postProcessCmds}
        rm -rf ${sysWorkDir}
    `;

    const wrapperPath = `${fsWorkDir}/proc.sh`;
    await $fs.write(wrapperPath, wrapperScript);
    await $cmd.run("chmod", ["+x", `storage/system/tmp/${wrapperPath}`]);

    const job = await $cmd.spawn("bash", [`storage/system/tmp/${wrapperPath}`], {
        timeout: 3600000,
        onProgress: {
            regex: ".*(time=[0-9:.]+|frame=.*fps=.*size=.*).*",
            channel: jobChannel,
            event: "progress"
        }
    });

    return new Response({
        success: true,
        job_id: job.pid,
        channel: jobChannel,
        ladder_used: ladderProfiles,
        codec_used: finalCodec,
        outputs: {
            hls: (format === 'hls' || format === 'both') ? `${output_name}/hls/master.m3u8` : null,
            dash: (format === 'dash' || format === 'both') ? `${output_name}/dash/manifest.mpd` : null
        }
    });
}

// =====================================================
// 🔧 Helpers
// =====================================================

function buildAutoLadder(maxHeight, range) {
    const presets = [
        { h: 144, b: "200k" },
        { h: 240, b: "400k" },
        { h: 360, b: "800k" },
        { h: 480, b: "1200k" },
        { h: 720, b: "2500k" },
        { h: 1080, b: "4500k" },
        { h: 1440, b: "8000k" },
        { h: 2160, b: "14000k" }
    ];

    let min = 144, max = maxHeight;

    if (range !== "auto") {
        const p = range.split("-");
        min = parseInt(p[0]);
        max = parseInt(p[1]);
    }

    return presets.filter(p => p.h >= min && p.h <= max && p.h <= maxHeight);
}

function getCodec(codec, gpu) {
    if (!codec) return ["-c", "copy"];

    if (gpu && (codec === "h264" || codec === "hevc")) {
        return codec === "h264"
            ? ["-c:v", "h264_nvenc"]
            : ["-c:v", "hevc_nvenc"];
    }

    const cpuMap = {
        h264: ["-c:v", "libx264", "-preset", "veryfast"],
        hevc: ["-c:v", "libx265", "-preset", "medium"],
        av1: ["-c:v", "libsvtav1", "-preset", "8"],
        // VP9 optimized for speed: realtime deadline + row-based multithreading
        vp9: ["-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "4", "-row-mt", "1"]
    };

    return cpuMap[codec] || ["-c", "copy"];
}

async function generateStorageCommands(rclone, jobChannel, sysWorkDir, sysDestDir, output_name, format) {
    if (rclone) {
        const conf = `
[target]
type=s3
provider=${rclone.provider || "AWS"}
access_key_id=${rclone.access_key}
secret_access_key=${rclone.secret_key}
region=${rclone.region}
endpoint=${rclone.endpoint || ""}
acl=public-read
`;
        await $fs.write(`${jobChannel}/rclone.conf`, conf);

        return `
            echo "Uploading to S3..."
            # For HLS (folder structure) use copy, for single file use copyto
            if [ "${format}" = "hls" ] || [ "${format}" = "dash" ] || [ "${format}" = "both" ]; then
                rclone --config ${sysWorkDir}/rclone.conf copy ${sysWorkDir} target:${rclone.bucket}/${output_name} --exclude "rclone.conf" --exclude "ffmpeg.log" --exclude "proc.sh"
            else
                # Single File
                FILENAME=$(ls ${sysWorkDir} | grep -vE 'proc.sh|rclone.conf|ffmpeg.log' | head -n 1)
                rclone --config ${sysWorkDir}/rclone.conf copyto ${sysWorkDir}/$FILENAME target:${rclone.bucket}/${output_name}/${output_name}.${format}
            fi
        `;
    } else {
        return `
            echo "Moving to Tenant Storage..."
            mkdir -p ${sysDestDir}
            cp -r ${sysWorkDir}/* ${sysDestDir}/
            # Remove artifacts
            rm -f ${sysDestDir}/ffmpeg.log ${sysDestDir}/proc.sh ${sysDestDir}/rclone.conf
        `;
    }
}
```

---

## 1. Usage

Call this service from your own backend script using `$run.script()`.

```javascript
const result = await $run.script("system-media-processor", {
    input_file: "my_video.mp4",  // Must be in your 'uploads' folder
    output_name: "processed_video",
    format: "mp4",               // mp4, webm, gif, mp3, hls
    quality: "medium"            // low, medium, high, original
});
```

### Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `input_file` | `string` | **Required** | Filename of a file in your `uploads` folder, OR a full public HTTP URL. |
| `output_name` | `string` | **Required** | Name of the output file/folder. |
| `format` | `string` | `"mp4"` | Target format. Options: `mp4`, `webm`, `gif`, `mp3`, `hls`. |
| `quality` | `string` | `"medium"` | Quality preset. See table below. |
| `rclone` | `object` | `null` | Optional S3 credentials to upload results directly to cloud storage. |

### Quality Presets

| Preset | Resolution | Video Bitrate | Audio Bitrate |
| :--- | :--- | :--- | :--- |
| `low` | 480p | 800k | 96k |
| `medium` | 720p | 2500k | 128k |
| `high` | 1080p | 5000k | 192k |
| `original` | Source | Copy | Copy |

---

## 2. Handling Results

The service runs asynchronously in the background. It returns immediately with a `job_id`.

**Response:**
```json
{
  "success": true,
  "job_id": 12345,
  "status": "processing",
  "output": "processed_video_medium.mp4"
}
```

### Accessing Output Files
Processed files are saved to your tenant's **`tmp`** directory. You can use `$fs` to access them.

```javascript
// Example: Moving the processed file to uploads after completion
// (You would typically do this in a scheduled job checking status or via a webhook pattern)

// Check if file exists in tmp
const exists = await $fs.exists("tmp/processed_video_medium.mp4");

if (exists) {
    // Read from tmp
    const b64 = await $fs.read("tmp/processed_video_medium.mp4"); // Warning: Be careful with large files in RAM
    
    // Save to uploads permanently
    await $zip.saveFile("final_video.mp4", b64, "video/mp4");
}
```

> **Note:** The `tmp` directory is ephemeral. Files might be cleaned up by system maintenance jobs. Move important files to `uploads` or external storage.

---

## 3. Advanced: Push to S3 (Rclone)

To avoid filling up your local disk, you can instruct the processor to upload directly to an S3 bucket using Rclone.

```javascript
await $run.script("system-media-processor", {
    input_file: "large_movie.mov",
    output_name: "stream",
    format: "hls",
    rclone: {
        provider: "AWS", // or Minio, DigitalOcean, etc.
        access_key: "...",
        secret_key: "...",
        region: "us-east-1",
        bucket: "my-streaming-bucket",
        path: "videos/processed" // Subfolder in bucket
    }
});
```
*In this mode, no files are saved to your local `tmp` folder.*

---

create a production-grade **ImageMagick Processor** as a Public Root Script. This tool will allow tenants to perform advanced image manipulations (resize, crop, watermark, format conversion) that go beyond simple thumbnails.

### Capabilities
1.  **Format Conversion**: Supports `jpg`, `png`, `webp`, `avif`, `tiff`.
2.  **Resizing & Cropping**: Smart resizing (`fit`, `fill`, `limit`), cropping, and gravity control.
3.  **Optimization**: Quality control and strip metadata.
4.  **Watermarking**: Overlay another image with transparency and position control.
5.  **Smart Storage**: Input from Tenant Uploads / Output to Tenant Tmp (or Rclone S3).

### Root Script: `system-image-processor`

**Configuration:**
*   **Visibility:** `public`
*   **Trigger:** `manual`

```javascript
/**
 * ApexKit Ultimate Image Processor
 * Wraps ImageMagick (magick) for advanced image manipulation.
 * 
 * @payload {string} input_file - Filename in tenant uploads OR http URL.
 * @payload {string} output_name - Desired output filename stem.
 * @payload {string} [format="webp"] - Output format.
 * @payload {object} [ops] - Operations config.
 * @payload {object} [ops.resize] - { width, height, fit: "cover"|"contain"|"fill" }
 * @payload {object} [ops.watermark] - { file: "logo.png", gravity: "southeast", opacity: 30, scale: 20 }
 * @payload {number} [quality=85] - Compression quality (1-100).
 */
export default async function (req) {
    const body = await req.json();
    const {
        input_file,
        output_name,
        format = "webp",
        ops = {},
        quality = 85,
        rclone
    } = body;

    // 1. Resolve Tenant Scope
    let tenantId = "root_test";
    if (body.__caller_scope && body.__caller_scope.Tenant) {
        tenantId = body.__caller_scope.Tenant;
    } else if (body.tenant_id) {
        tenantId = body.tenant_id;
    }

    const jobId = $util.uuid().split('-')[0];
    const fsWorkDir = `./img_job_${jobId}`;
    const sysWorkDir = `storage/system/tmp/img_job_${jobId}`;

    console.log(`[Image] Job ${jobId} for Tenant ${tenantId}`);

    // 2. Resolve Input
    let sysInputPath = input_file;
    if (!input_file.startsWith("http")) {
        sysInputPath = `storage/tenants/${tenantId}/uploads/${input_file}`;
        // Note: Watermark file path logic below assumes tenant upload too
    }

    // 3. Prepare Command Arguments
    // We use "magick" (IM v7) syntax
    let args = [sysInputPath];

    // --- OPERATIONS ---

    // Resize
    if (ops.resize) {
        const { width, height, fit = "cover" } = ops.resize;
        let geo = `${width || ''}x${height || ''}`;

        if (fit === "cover") geo += "^";   // Fill area, crop needed later?
        else if (fit === "contain") geo += ""; // Default
        else if (fit === "fill") geo += "!";   // Stretch
        else if (fit === "limit") geo += ">";  // Shrink only

        args.push("-resize", geo);

        if (fit === "cover" && width && height) {
            // Smart center crop after resize
            args.push("-gravity", "center", "-extent", `${width}x${height}`);
        }
    }

    // Rotate
    if (ops.rotate) {
        args.push("-rotate", ops.rotate.toString());
    }

    // Effects
    if (ops.blur) args.push("-blur", `0x${ops.blur}`);
    if (ops.grayscale) args.push("-colorspace", "Gray");

    // Optimization
    args.push("-quality", quality.toString());
    args.push("-strip"); // Remove EXIF

    // 4. Handle Watermark (Composite)
    // Complex because we need to load the second image into the stack
    let watermarkCmd = "";
    if (ops.watermark) {
        // We handle watermark by creating a composite command part or using a second convert execution?
        // IM v7 allows complex stacks.
        // Format: input -resize ... null: watermark -resize ... -gravity ... -composite output

        let wmPath = ops.watermark.file;
        if (!wmPath.startsWith("http")) {
            wmPath = `storage/tenants/${tenantId}/uploads/${wmPath}`;
        }

        // Add watermark to stack
        args.push(wmPath);

        // Resize watermark (relative scale?)
        if (ops.watermark.scale) {
            args.push("-resize", `${ops.watermark.scale}%`);
        }

        // Opacity (via alpha channel)
        if (ops.watermark.opacity) {
            // "channel a -evaluate multiply 0.5 +channel" logic for opacity is complex in CLI
            // Easier: "-dissolve" if using "composite" command, but we are using "magick".
            // IM v7: -alpha set -channel A -evaluate multiply 0.3
            const opac = ops.watermark.opacity / 100.0;
            args.push("-alpha", "set", "-channel", "A", "-evaluate", "multiply", opac.toString(), "+channel");
        }

        const gravity = ops.watermark.gravity || "southeast";
        const padding = ops.watermark.padding || "+10+10";
        args.push("-gravity", gravity, "-geometry", padding, "-composite");
    }

    // 5. Output
    const outputFilename = `${output_name}.${format}`;
    const sysOutputPath = `${sysWorkDir}/${outputFilename}`;
    args.push(sysOutputPath);

    // 6. Execution Wrapper
    let sysDestDir = `storage/tenants/${tenantId}/tmp`;
    if (tenantId === "root_test") sysDestDir = `storage/tmp/root_test`;

    await $fs.mkdir(fsWorkDir);

    // Shell Escape Helper
    const shellEscape = (arg) => "'" + arg.replace(/'/g, "'\\''") + "'";
    const escapedArgs = args.map(shellEscape);

    // Destination Logic (Local or Rclone)
    let postProcess = `
        mkdir -p ${sysDestDir}
        mv ${sysWorkDir}/${outputFilename} ${sysDestDir}/
    `;

    if (rclone) {
        // ... (Reuse Rclone logic from media processor) ...
        // Simplified for brevity:
        const rcloneConfig = `[target]\ntype=s3\n...`; // Fill from rclone obj
        await $fs.write(`${fsWorkDir}/rclone.conf`, rcloneConfig);
        const dest = `${rclone.bucket}/${rclone.path || ''}/${outputFilename}`;
        postProcess = `rclone --config ${sysWorkDir}/rclone.conf copyto ${sysOutputPath} target:${dest}`;
    }

    const wrapperScript = `
        #!/bin/bash
        magick ${escapedArgs.join(" ")} > ${sysWorkDir}/magick.log 2>&1
        if [ $? -eq 0 ]; then
            ${postProcess}
        else
            echo "ImageMagick Failed"
            cat ${sysWorkDir}/magick.log
        fi
        rm -rf ${sysWorkDir}
    `;

    const sysScriptPath = `${sysWorkDir}/proc.sh`;
    await $fs.write(`./img_job_${jobId}/proc.sh`, wrapperScript);
    await $cmd.run("chmod", ["+x", sysScriptPath]);

    const job = await $cmd.spawn("bash", [sysScriptPath], { timeout: 60000 });

    return new Response({
        success: true,
        job_id: job.pid,
        output: outputFilename
    });
}
```

### Usage Example

```javascript
// Tenant Script
await $run.script("system-image-processor", {
    input_file: "large_photo.jpg",
    output_name: "profile_optimized",
    format: "webp",
    ops: {
        resize: { width: 800, height: 800, fit: "cover" },
        watermark: { 
            file: "logo.png", 
            gravity: "southeast", 
            opacity: 50, 
            scale: 15 
        },
        grayscale: true
    }
});
```

---

# Advanced Image Processor

This is a highly advanced, dedicated **Image Processing Edge Function**. It integrates three powerful command-line tools that you must have installed on your server:

1.  **ImageMagick (`magick`)**: For cropping, resizing, formatting, and watermarking.
2.  **Rembg (`rembg`)**: An AI tool for removing backgrounds (using U2Net). *Requires Python/pip install*.
3.  **Real-ESRGAN (`realsr-ncnn-vulkan` or `realesrgan-ncnn-vulkan`)**: A tiny, blazing-fast AI upscaler. *Requires downloading the binary release*.

### 1. Prerequisites (Server Setup)
Run this on your host machine to install the required tools:
```bash
# 1. ImageMagick
sudo apt-get install -y imagemagick

# 2. Rembg (Background Removal AI)
sudo apt-get install -y python3-pip
pip3 install rembg[cli]
pip3 install "rembg[cpu]" OR "rembg[gpu]"
# Ensure ~/.local/bin is in your PATH

# 3. Real-ESRGAN (AI Upscaler - Portable Binary)
wget https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/realesrgan-ncnn-vulkan-20220424-ubuntu.zip
unzip realesrgan-ncnn-vulkan-20220424-ubuntu.zip -d realesrgan
sudo mv realesrgan/realesrgan-ncnn-vulkan /usr/local/bin/
sudo mv realesrgan/models /usr/local/bin/
```

### 2. Root Script: `advanced-system-image-processor`

**Configuration:**
*   **Name**: `advanced-system-image-processor`
*   **Trigger**: `manual`
*   **Visibility**: `public`

**Code:**

```javascript
/**
 * ApexKit Ultimate Image Processor v1.0
 * 
 * Pipeline Execution Order:
 * 1. AI Upscale (if requested)
 * 2. AI Background Removal (if requested)
 * 3. ImageMagick Processing (Resize, Crop, Watermark, Format)
 * 
 * @param {string} input_file - Filename in tenant uploads OR http(s) URL
 * @param {string} output_name - Desired output filename stem
 * @param {string} [format="webp"] - "webp", "jpg", "png", "avif"
 * @param {object} [ops] - Dictionary of operations
 * @param {boolean} [ops.remove_bg=false] - AI Background removal (rembg)
 * @param {boolean} [ops.upscale=false] - AI Upscale 4x (realesrgan)
 * @param {object} [ops.resize] - { w: 800, h: 800, fit: "cover"|"contain" }
 * @param {number} [ops.blur=0] - Gaussian blur radius
 * @param {object} [rclone=null] - S3 upload config
 */
export default async function (req) {
    const body = await req.json();

    const {
        input_file,
        output_name,
        format = "webp",
        ops = {},
        rclone = null
    } = body;

    // 1. Resolve Tenant Scope
    let tenantId = "root_test";
    if (body.__caller_scope && body.__caller_scope.Tenant) {
        tenantId = body.__caller_scope.Tenant;
    } else if (body.tenant_id) {
        tenantId = body.tenant_id;
    }

    if (!input_file || !output_name)
        return new Response({ success: false, error: "Missing input_file or output_name" }, { status: 400 });

    const jobId = $util.uuid().split("-")[0];
    const jobChannel = tenantId === "root_test" ? `root::img_${jobId}` : `tenant_${tenantId}::img_${jobId}`;

    const localJobId = `img_job_${jobId}`;
    const fsWorkDir = `${localJobId}`;
    const sysWorkDir = `storage/system/tmp/${localJobId}`;

    await $fs.mkdir(fsWorkDir);

    const inputPath = input_file.startsWith("http") ? input_file : `storage/tenants/${tenantId}/uploads/${input_file}`;
    let sysDestDir = tenantId === "root_test" ? `storage/tmp/root_test` : `storage/tenants/${tenantId}/tmp`;

    const finalOutput = `${output_name}.${format}`;

    // =====================================================
    // 🛠️ BUILD PIPELINE SCRIPT WITH LOGGING
    // =====================================================

    let pipeline = `
#!/bin/bash
# Ensure Python bins and local bins are in PATH
export PATH=$PATH:$HOME/.local/bin:/usr/local/bin:/opt/bin

LOG_FILE="${sysWorkDir}/process.log"
DEST_DIR="${sysDestDir}"
FINAL_OUTPUT="${finalOutput}"

# Cleanup Trap: Always runs on exit (success or fail)
cleanup() {
    EXIT_CODE=$?
    mkdir -p "$DEST_DIR"
    
    # Always save the log file for debugging
    cp "$LOG_FILE" "$DEST_DIR/${localJobId}_debug.log"
    
    if [ $EXIT_CODE -ne 0 ]; then
        echo "Pipeline FAILED. Check log: $DEST_DIR/${localJobId}_debug.log" >&2
    else
        echo "Pipeline SUCCESS." >&2
    fi
    
    rm -rf "${sysWorkDir}"
    exit $EXIT_CODE
}

trap cleanup EXIT

# Run operations in a subshell, exit on error (-e), and print commands (-x)
(
    set -ex
    echo "Starting Image Pipeline..."
    
    # 1. Detect ImageMagick Command
    if command -v magick >/dev/null 2>&1; then
        IM_CMD="magick"
    elif command -v convert >/dev/null 2>&1; then
        IM_CMD="convert"
    else
        echo "ERROR: ImageMagick not found!"
        exit 1
    fi

    # 2. Get Input File
    CURRENT_FILE="${sysWorkDir}/input_raw"
    if [[ "${inputPath}" == http* ]]; then
        curl -sL "${inputPath}" -o "$CURRENT_FILE"
    else
        cp "${inputPath}" "$CURRENT_FILE"
    fi
    `;

    // --- STEP 1: AI UPSCALE ---
    if (ops.upscale) {
        pipeline += `
    echo "Processing: AI Fast Upscale (4x)..."
    # Using the tiny 'animevideov3' model which is already installed and lightning fast
    realesrgan-ncnn-vulkan -i "$CURRENT_FILE" -o "${sysWorkDir}/upscaled.png" -n realesr-animevideov3-x4 -s 4 -j 4:4 -m models
    CURRENT_FILE="${sysWorkDir}/upscaled.png"
        `;
    }

    // --- STEP 2: AI BACKGROUND REMOVAL ---
    if (ops.remove_bg) {
        pipeline += `
    echo "Processing: AI Background Removal..."
    rembg i "$CURRENT_FILE" "${sysWorkDir}/nobg.png"
    CURRENT_FILE="${sysWorkDir}/nobg.png"
        `;
    }

    // --- STEP 3: IMAGEMAGICK (Resize, Crop, Format) ---
    let imArgs = ["\"$IM_CMD\"", "\"$CURRENT_FILE\""];

    // Force ImageMagick to use a transparent canvas for any padding/cropping
    imArgs.push("-background", "transparent");

    if (ops.resize) {
        const w = ops.resize.w || '';
        const h = ops.resize.h || '';
        const fit = ops.resize.fit || 'contain';

        if (fit === 'cover' && w && h) {
            imArgs.push("-resize", `"${w}x${h}^"`, "-gravity", "center", "-extent", `"${w}x${h}"`);
        } else if (fit === 'fill') {
            imArgs.push("-resize", `"${w}x${h}!"`);
        } else {
            imArgs.push("-resize", `"${w}x${h}"`);
        }
    }

    if (ops.blur) imArgs.push("-blur", `"0x${ops.blur}"`);

    imArgs.push("-strip");
    imArgs.push("-quality", (ops.quality || 85).toString());
    imArgs.push(`"${sysWorkDir}/${finalOutput}"`);

    pipeline += `
    echo "Processing: Image Formatting..."
    ${imArgs.join(" ")}
    `;

    // --- STEP 4: STORAGE ROUTING ---
    if (rclone) {
        const conf = `[target]\ntype=s3\nprovider=${rclone.provider || "AWS"}\naccess_key_id=${rclone.access_key}\nsecret_access_key=${rclone.secret_key}\nregion=${rclone.region}\nendpoint=${rclone.endpoint || ""}\nacl=public-read`;
        await $fs.write(`${localJobId}/rclone.conf`, conf);

        pipeline += `
    echo "Uploading to S3..."
    rclone --config ${sysWorkDir}/rclone.conf copyto ${sysWorkDir}/${finalOutput} target:${rclone.bucket}/${finalOutput}
        `;
    } else {
        pipeline += `
    echo "Moving to Tenant Storage..."
    mv ${sysWorkDir}/${finalOutput} $DEST_DIR/
        `;
    }

    pipeline += `
) 2>&1 | tee "$LOG_FILE" >&2
    `;

    // Write and Execute
    const wrapperPath = `${fsWorkDir}/proc.sh`;
    await $fs.write(wrapperPath, pipeline);
    await $cmd.run("chmod", ["+x", `storage/system/tmp/${wrapperPath}`]);

    const job = await $cmd.spawn("bash", [`storage/system/tmp/${wrapperPath}`], {
        timeout: 180000, // 2 mins max
        onProgress: {
            regex: "^Processing: (.*)",
            channel: jobChannel,
            event: "progress"
        }
    });

    return new Response({
        success: true,
        job_id: job.pid,
        status: job.status,
        channel: `img_${jobId}`,
        output: finalOutput
    });
}
```

### 3. Usage Example

You can call this from any tenant script. Here is an example of taking a low-resolution product image, removing the background, upscaling it, and saving it as an optimized WebP.

```javascript
const result = await $run.script("advanced-system-image-processor", {
    input_file: "blurry_product.jpg",
    output_name: "product_hero",
    format: "webp",
    ops: {
        upscale: true,       // Makes it 4x resolution using AI
        remove_bg: true,     // Removes background using AI
        resize: { w: 1080, h: 1080, fit: "cover" }, // Crops to perfect square
        quality: 90
    }
});

// Returns instantly. Monitor via SSE or check `tmp` folder in ~120 seconds.
```