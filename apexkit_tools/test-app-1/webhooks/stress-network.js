/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "id": 4,
  "name": "stress-network",
  "extension": "js",
  "target_collection": null,
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "manual",
  "active": true,
  "visibility": "private"
};

export default async function (req) {
    console.log("[Stress Test] Initiating HTTP Flood...");

    const promises = [];
    // Flood the engine with 50 parallel requests to a slow endpoint
    for (let i = 0; i < 50; i++) {
        promises.push(
            fetch("https://httpbin.org/delay/5", { timeout: 10000 })
                .then(res => res.status)
                .catch(err => err.message)
        );
    }

    const results = await Promise.all(promises);

    return new Response(JSON.stringify({ 
        message: "Flood completed. Check results to see which were queued, succeeded, or aborted.",
        results 
    }), { 
        headers: { "Content-Type": "application/json" }
    });
}