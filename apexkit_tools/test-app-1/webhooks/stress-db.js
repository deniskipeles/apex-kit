/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "id": 2,
  "name": "stress-db",
  "extension": "js",
  "target_collection": null,
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "manual",
  "active": true,
  "visibility": "private"
};

export default async function (req) {
    console.log("[Stress Test] Hammering the database...");
    const startTime = Date.now();

    const ids = [];
    // 1. Rapid Sequential Inserts
    for (let i = 0; i < 100; i++) {
        const res = await $db.records.create("pins", {
            title: `Stress Test Pin ${i}`,
            description: "Testing database mutex locks",
            likes_count: i
        });
        ids.push(res.id);
    }

    // 2. High-Speed Reads & Updates (Testing the O(1) Index lookup fix)
    for (const id of ids) {
        await $db.records.update("pins", id, { likes_count: 9999 });
        await $db.records.get("pins", id); 
    }

    // 3. Cleanup
    for (const id of ids) {
        await $db.records.delete("pins", id);
    }

    const duration = Date.now() - startTime;

    return new Response(JSON.stringify({ 
        message: "Database Hammer complete.",
        records_processed: 100,
        time_taken_ms: duration
    }), { 
        headers: { "Content-Type": "application/json" }
    });
}

