/** @type {import("../apexkit").FileMetadata} */
export const __fileMetadata__ = {
  "id": 1,
  "name": "stress-cpu",
  "extension": "js",
  "target_collection": null,
  "type": "webhook",
  "path": "./webhooks/",
  "trigger_type": "manual",
  "active": true,
  "visibility": "private"
};

export default async function (req) {
    console.log("[Stress Test] Initiating Infinite CPU Loop...");
    
    let counter = 0;
    // Malicious infinite loop trying to lock the thread
    while (true) {
        counter += Math.sqrt(Math.random() * 1000000);
        
        // Just to prevent extreme compiler optimizations from skipping the loop
        if (counter < 0) break; 
    }

    return new Response("You will never see this message if the scheduler works.", { status: 200 });
}